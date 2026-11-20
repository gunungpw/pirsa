//! Launcher state and iced update logic.

use std::time::{Duration, Instant};

use iced::futures::{StreamExt, stream};
use iced::keyboard::key::Named;
use iced::keyboard::{Event as KeyboardEvent, Key, Modifiers};
use iced::widget::operation;
use iced::{Element, Event, Subscription, Task, window};
use tokio::sync::broadcast::error::RecvError;

use crate::platform::{self, AppEntry, HotkeyStatus};
use crate::search;
use crate::ui;

/// The application's complete state.
pub struct Launcher {
    pub(crate) apps: Vec<AppEntry>,
    pub(crate) results: Vec<usize>,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) window_id: Option<window::Id>,
    pub(crate) hwnd: Option<isize>,
    pub(crate) hotkey: HotkeyStatus,
    pub(crate) error: Option<String>,
    pub(crate) warning: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    QueryChanged(String),
    ClearQuery,
    Navigate(i32),
    Activate(usize),
    Hide,
    Quit,
    DragWindow,
    Platform,
    Tick,
    EnsureFocus,
    WindowReady(window::Id, isize),
    Event(Event),
}

impl Launcher {
    pub fn boot() -> (Self, Task<Message>) {
        let started = Instant::now();
        let scan = platform::scan();
        let elapsed = started.elapsed();

        crate::util::log(format!(
            "indexed {} apps in {} ms",
            scan.apps.len(),
            elapsed.as_millis()
        ));

        let error = if scan.apps.is_empty() {
            Some("No Start Menu applications were found.".to_owned())
        } else {
            None
        };

        let mut launcher = Self {
            apps: scan.apps,
            results: Vec::new(),
            query: String::new(),
            selected: 0,
            visible: platform::start_visible(),
            focused: false,
            window_id: None,
            hwnd: None,
            hotkey: platform::hotkey_status(),
            error,
            warning: scan.warning,
        };
        launcher.recompute();

        let ready: Task<Message> = window::oldest().and_then(|id| {
            window::raw_id::<Message>(id).map(move |raw| Message::WindowReady(id, raw as isize))
        });

        let task = if launcher.visible {
            Task::batch([ready, focus_input()])
        } else {
            ready
        };

        (launcher, task)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueryChanged(query) => {
                self.query = query;
                self.error = None;
                self.recompute();
                self.selected = 0;
                Task::none()
            }
            Message::ClearQuery => {
                self.query.clear();
                self.error = None;
                self.recompute();
                Task::none()
            }
            Message::Navigate(delta) => {
                if !self.results.is_empty() {
                    let last = self.results.len() - 1;
                    self.selected = if delta < 0 {
                        self.selected.saturating_sub(delta.unsigned_abs() as usize)
                    } else {
                        self.selected.saturating_add(delta as usize).min(last)
                    };
                }

                Task::none()
            }
            Message::Activate(index) => self.activate(index),
            Message::Hide => self.hide(),
            Message::Quit => iced::exit(),
            Message::DragWindow => self
                .window_id
                .map(window::drag::<Message>)
                .unwrap_or_else(Task::none),
            Message::Platform | Message::Tick => self.tick(),
            Message::EnsureFocus => self.ensure_focus(),
            Message::WindowReady(id, hwnd) => {
                self.window_id = Some(id);
                self.hwnd = Some(hwnd);
                crate::util::log(format!("window ready (hwnd={hwnd:#x})"));

                if self.visible {
                    if !platform::window::is_visible(hwnd) {
                        platform::window::show(hwnd);
                    }

                    Task::batch([
                        window::set_mode::<Message>(id, window::Mode::Windowed),
                        window::gain_focus::<Message>(id),
                        focus_input(),
                        ensure_focus_later(),
                    ])
                } else {
                    Task::none()
                }
            }
            Message::Event(event) => self.handle_event(event),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let platform_signals = Subscription::run(|| {
            let receiver = platform::subscribe();

            // Check any request that arrived before the UI was ready, then
            // wait without polling.
            let initial = stream::once(async { Message::Tick });
            let updates = stream::unfold(receiver, |mut receiver| async move {
                match receiver.recv().await {
                    Ok(()) => Some((Message::Platform, receiver)),
                    Err(RecvError::Lagged(_)) => Some((Message::Tick, receiver)),
                    Err(RecvError::Closed) => None,
                }
            });

            initial.chain(updates)
        });

        Subscription::batch([
            platform_signals,
            iced::event::listen_with(|event, _status, _window| match event {
                Event::Keyboard(_) | Event::Window(_) => Some(Message::Event(event)),
                _ => None,
            }),
        ])
    }

    fn tick(&mut self) -> Task<Message> {
        let (show, toggle) = platform::take_requests();

        if show {
            return self.show();
        }

        if toggle {
            return self.toggle();
        }

        Task::none()
    }

    fn toggle(&mut self) -> Task<Message> {
        let actually_visible = self
            .hwnd
            .map(platform::window::is_visible)
            .unwrap_or(self.visible);

        let foreground = self
            .hwnd
            .map(platform::window::is_foreground)
            .unwrap_or(false);

        crate::util::log(format!(
            "toggle requested (visible={actually_visible}, foreground={foreground})"
        ));

        if actually_visible && foreground {
            self.hide()
        } else {
            self.show()
        }
    }

    fn show(&mut self) -> Task<Message> {
        if let Some(hwnd) = self.hwnd {
            platform::window::show(hwnd);
        }

        self.visible = true;
        self.focused = false;
        self.query.clear();
        self.error = None;
        self.recompute();
        self.selected = 0;

        let mut tasks = Vec::with_capacity(3);

        if let Some(id) = self.window_id {
            tasks.push(window::set_mode::<Message>(id, window::Mode::Windowed));
            tasks.push(window::gain_focus::<Message>(id));
        }

        tasks.push(focus_input());
        tasks.push(ensure_focus_later());
        Task::batch(tasks)
    }

    fn ensure_focus(&mut self) -> Task<Message> {
        if !self.visible {
            return Task::none();
        }

        if let Some(hwnd) = self.hwnd {
            platform::window::focus(hwnd);
        }

        focus_input()
    }

    fn hide(&mut self) -> Task<Message> {
        if let Some(hwnd) = self.hwnd {
            platform::window::hide(hwnd);
        }

        self.visible = false;
        self.focused = false;
        self.query.clear();
        self.error = None;
        self.recompute();
        self.selected = 0;

        self.window_id
            .map(|id| window::set_mode::<Message>(id, window::Mode::Hidden))
            .unwrap_or_else(Task::none)
    }

    fn activate(&mut self, index: usize) -> Task<Message> {
        let Some(&app_index) = self.results.get(index) else {
            return Task::none();
        };

        let app = &self.apps[app_index];

        match platform::launch(app) {
            Ok(()) => self.hide(),
            Err(error) => {
                let name = app.name.clone();
                self.error = Some(format!("Could not launch {name}: {error}"));
                crate::util::log(format!("launch failed for {name}: {error}"));
                Task::none()
            }
        }
    }

    fn rescan(&mut self) {
        let scan = platform::scan();
        let count = scan.apps.len();

        self.apps = scan.apps;
        self.warning = scan.warning;
        self.error = if count == 0 {
            Some("No Start Menu applications were found.".to_owned())
        } else {
            None
        };
        self.recompute();
        self.selected = 0;

        crate::util::log(format!("rescanned Start Menu ({count} apps)"));
    }

    fn recompute(&mut self) {
        self.results = search::rank(&self.apps, &self.query);

        if self.selected >= self.results.len() {
            self.selected = self.results.len().saturating_sub(1);
        }
    }

    fn handle_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Keyboard(KeyboardEvent::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) => self.handle_key(key, modifiers, repeat),
            Event::Window(window::Event::Focused) => {
                self.focused = true;

                if self.visible {
                    focus_input()
                } else {
                    Task::none()
                }
            }
            Event::Window(window::Event::Unfocused) => {
                if self.visible && self.focused {
                    self.hide()
                } else {
                    self.focused = false;
                    Task::none()
                }
            }
            Event::Window(window::Event::CloseRequested) => self.hide(),
            _ => Task::none(),
        }
    }

    fn handle_key(&mut self, key: Key, modifiers: Modifiers, repeat: bool) -> Task<Message> {
        let key = key.as_ref();

        if modifiers.control() && matches!(key, Key::Character("q") | Key::Character("Q")) {
            return iced::exit();
        }

        if repeat {
            return match key {
                Key::Named(Named::ArrowUp) => self.update(Message::Navigate(-1)),
                Key::Named(Named::ArrowDown) => self.update(Message::Navigate(1)),
                _ => Task::none(),
            };
        }

        match key {
            Key::Named(Named::Escape) => self.hide(),
            Key::Named(Named::ArrowUp) => self.update(Message::Navigate(-1)),
            Key::Named(Named::ArrowDown) => self.update(Message::Navigate(1)),
            Key::Named(Named::Enter) => self.activate(self.selected),
            Key::Named(Named::F5) => {
                self.rescan();
                Task::none()
            }
            _ => Task::none(),
        }
    }
}

fn focus_input() -> Task<Message> {
    operation::focus::<Message>(ui::INPUT_ID)
}

fn ensure_focus_later() -> Task<Message> {
    Task::future(async {
        tokio::time::sleep(Duration::from_millis(140)).await;
        Message::EnsureFocus
    })
}
