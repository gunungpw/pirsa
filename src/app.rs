//! Launcher state and iced update logic.

use std::collections::HashMap;
use std::time::Duration;

use iced::futures::{StreamExt, stream};
use iced::keyboard::key::Named;
use iced::keyboard::{Event as KeyboardEvent, Key, Modifiers};
use iced::widget::image::Handle;
use iced::widget::operation;
use iced::{Element, Event, Subscription, Task, window};

use crate::platform::{self, AppEntry, HotkeyStatus, ScanResult};
use crate::search;
use crate::ui;
use crate::usage::Usage;

/// The application's complete state.
pub struct Launcher {
    pub(crate) apps: Vec<AppEntry>,
    pub(crate) results: Vec<usize>,
    /// Launch history: how often each app was opened, used for ordering.
    pub(crate) usage: Usage,
    /// Real application icons, keyed by display name.
    pub(crate) icons: HashMap<String, Handle>,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) window_id: Option<window::Id>,
    pub(crate) hwnd: Option<isize>,
    pub(crate) hotkey: HotkeyStatus,
    pub(crate) error: Option<String>,
    pub(crate) warning: Option<String>,
    pub(crate) indexing: bool,
    /// Monotonic token for the newest scan; older results are discarded.
    pub(crate) scan_generation: u64,
    /// Monotonic token for the newest icon batch; older results are discarded.
    pub(crate) icon_generation: u64,
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
    EnsureFocus,
    WindowReady(window::Id, isize),
    ScanComplete {
        generation: u64,
        scan: ScanResult,
    },
    IconsLoaded {
        generation: u64,
        icons: Vec<(String, platform::Icon)>,
    },
    Event(Event),
}

impl Launcher {
    pub fn boot() -> (Self, Task<Message>) {
        let mut launcher = Self {
            apps: Vec::new(),
            results: Vec::new(),
            usage: Usage::load(),
            icons: HashMap::new(),
            query: String::new(),
            selected: 0,
            visible: platform::start_visible(),
            focused: false,
            window_id: None,
            hwnd: None,
            hotkey: platform::hotkey_status(),
            error: None,
            warning: None,
            indexing: true,
            scan_generation: 0,
            icon_generation: 0,
        };

        let ready: Task<Message> = window::oldest().and_then(|id| {
            window::raw_id::<Message>(id).map(move |raw| Message::WindowReady(id, raw as isize))
        });

        // Do not block window creation on the Start Menu scan. The window
        // appears immediately while the shell worker indexes in the background.
        let scan = launcher.begin_scan();

        let task = if launcher.visible {
            Task::batch([ready, focus_input(), scan])
        } else {
            Task::batch([ready, scan])
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
                self.selected = 0;
                Task::none()
            }
            Message::Navigate(delta) => {
                if !self.results.is_empty() {
                    let last = self.results.len().min(ui::MAX_RESULTS) - 1;
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
            Message::Platform => self.tick(),
            Message::EnsureFocus => self.ensure_focus(),
            Message::ScanComplete { generation, scan } => self.apply_scan(generation, scan),
            Message::IconsLoaded { generation, icons } => {
                if generation != self.icon_generation {
                    return Task::none();
                }

                for (name, icon) in icons {
                    self.icons
                        .insert(name, Handle::from_rgba(icon.width, icon.height, icon.rgba));
                }

                Task::none()
            }
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
            // Deliver a first message so requests that arrived before the UI
            // was ready are handled, then wait for the next notification.
            stream::once(async { Message::Platform }).chain(stream::unfold(
                platform::subscribe(),
                |notifier| async move {
                    notifier.notified().await;
                    Some((Message::Platform, notifier))
                },
            ))
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
        let requests = platform::take_requests();

        if requests.quit {
            return iced::exit();
        }

        if requests.show {
            return self.show();
        }

        if requests.toggle {
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
        self.reset_search();

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
        self.reset_search();

        self.window_id
            .map(|id| window::set_mode::<Message>(id, window::Mode::Hidden))
            .unwrap_or_else(Task::none)
    }

    fn activate(&mut self, index: usize) -> Task<Message> {
        let Some(&app_index) = self.results.get(index) else {
            return Task::none();
        };

        let Some(app) = self.apps.get(app_index) else {
            return Task::none();
        };

        let key = app.search_key().to_owned();
        let name = app.name().to_owned();

        match platform::launch(app) {
            Ok(()) => {
                // Remember the launch before hiding so the next visit already
                // shows this app near the top of the list.
                self.usage.record(&key);
                self.usage.save();
                self.hide()
            }
            Err(error) => {
                self.error = Some(format!("Could not launch {name}: {error}"));
                crate::util::log(format!("launch failed for {name}: {error}"));
                Task::none()
            }
        }
    }

    /// Starts an asynchronous scan and tags it with a new generation.
    fn begin_scan(&mut self) -> Task<Message> {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        let generation = self.scan_generation;
        self.indexing = true;

        Task::perform(platform::scan(), move |scan| Message::ScanComplete {
            generation,
            scan,
        })
    }

    fn rescan(&mut self) -> Task<Message> {
        // The scan worker serializes scans, so queueing another one while a
        // scan is running only delays the result the user is waiting for.
        if self.indexing {
            return Task::none();
        }

        self.begin_scan()
    }

    /// Replaces the indexed apps and refreshes the visible results. Shared by
    /// the startup scan and F5 so both follow the same rules.
    fn apply_scan(&mut self, generation: u64, scan: ScanResult) -> Task<Message> {
        if generation != self.scan_generation {
            crate::util::log(format!(
                "ignoring stale scan {generation} (current {})",
                self.scan_generation
            ));
            return Task::none();
        }

        let count = scan.apps.len();

        self.apps = scan.apps;
        self.warning = scan.warning;
        self.indexing = false;
        self.error = if count == 0 {
            Some("No Start Menu applications were found.".to_owned())
        } else {
            None
        };
        self.recompute();
        self.selected = 0;

        crate::util::log(format!("indexed {count} apps"));
        self.load_icons()
    }

    /// Clears the query and any error, then rebuilds the result list.
    fn reset_search(&mut self) {
        self.query.clear();
        self.error = None;
        self.recompute();
        self.selected = 0;
    }

    /// Loads the real application icons on the shell worker.
    fn load_icons(&mut self) -> Task<Message> {
        self.icon_generation = self.icon_generation.wrapping_add(1);
        let generation = self.icon_generation;
        let apps = self.apps.clone();

        Task::perform(platform::extract_icons(apps), move |icons| {
            Message::IconsLoaded { generation, icons }
        })
    }

    fn recompute(&mut self) {
        self.results = search::rank_with_usage(&self.apps, &self.query, &self.usage);

        let visible = self.results.len().min(ui::MAX_RESULTS);

        if self.selected >= visible {
            self.selected = visible.saturating_sub(1);
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
            Key::Named(Named::F5) => self.rescan(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher_with_results(count: usize) -> Launcher {
        Launcher {
            apps: Vec::new(),
            results: (0..count).collect(),
            usage: Usage::default(),
            icons: HashMap::new(),
            query: String::new(),
            selected: 0,
            visible: true,
            focused: false,
            window_id: None,
            hwnd: None,
            hotkey: HotkeyStatus::Pending,
            error: None,
            warning: None,
            indexing: false,
            scan_generation: 0,
            icon_generation: 0,
        }
    }

    #[test]
    fn navigation_stays_within_visible_rows() {
        let mut launcher = launcher_with_results(20);

        let _ = launcher.update(Message::Navigate(100));
        assert_eq!(launcher.selected, ui::MAX_RESULTS - 1);

        let _ = launcher.update(Message::Navigate(-100));
        assert_eq!(launcher.selected, 0);
    }

    #[test]
    fn stale_scan_results_are_ignored() {
        let mut launcher = launcher_with_results(0);
        launcher.scan_generation = 2;
        launcher.indexing = true;

        let _ = launcher.apply_scan(
            1,
            ScanResult {
                apps: Vec::new(),
                warning: None,
            },
        );

        assert!(launcher.apps.is_empty());
        assert!(launcher.indexing);
    }

    #[test]
    fn rescan_is_ignored_while_indexing() {
        let mut launcher = launcher_with_results(0);
        launcher.scan_generation = 1;
        launcher.indexing = true;

        let _ = launcher.rescan();

        assert_eq!(launcher.scan_generation, 1);
    }
}
