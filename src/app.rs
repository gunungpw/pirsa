//! Launcher state and iced update logic.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use iced::futures::{StreamExt, stream};
use iced::keyboard::key::Named;
use iced::keyboard::{Event as KeyboardEvent, Key, Modifiers};
use iced::widget::image::Handle;
use iced::widget::operation;
use iced::{Element, Event, Font, Subscription, Task, Theme, window};

use pirsa_core::calc::{self, CalcResult};
use pirsa_core::files::{self as file_index, FileEntry, FileQuery};
use pirsa_core::search;
use pirsa_core::settings::{HotkeyChoice, Settings, ThemeChoice};
use pirsa_core::usage::Usage;
use pirsa_core::util;
use pirsa_core::{AppEntry, HotkeyStatus, Icon, ScanResult};
use pirsa_platform as platform;
use pirsa_ui as ui;

/// The application's complete state.
pub struct Launcher {
    pub(crate) apps: Vec<AppEntry>,
    pub(crate) results: Vec<usize>,
    /// Optional calculator/unit-conversion result for the current query.
    pub(crate) calc: Option<CalcResult>,
    /// Launch history: how often each app was opened, used for ordering.
    pub(crate) usage: Usage,
    /// Real application icons, keyed by display name.
    pub(crate) icons: HashMap<String, Handle>,
    /// Matches of the current `~` / `>` filesystem query.
    pub(crate) files: Vec<FileEntry>,
    pub(crate) file_results: Vec<usize>,
    /// Present while the search field holds a file-mode query.
    pub(crate) file_query: Option<FileQuery>,
    pub(crate) settings: Settings,
    pub(crate) theme: Theme,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) window_id: Option<window::Id>,
    pub(crate) settings_window: Option<window::Id>,
    pub(crate) hwnd: Option<isize>,
    pub(crate) hotkey: HotkeyStatus,
    /// Hotkey change currently round-tripping through the platform thread.
    pub(crate) hotkey_pending: Option<HotkeyChoice>,
    /// Feedback shown inside the settings window, such as a rejected hotkey.
    pub(crate) settings_notice: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) warning: Option<String>,
    pub(crate) indexing: bool,
    /// Set when F5 arrives while a scan is running; runs once the scan lands.
    pub(crate) rescan_queued: bool,
    /// Monotonic token for the newest file search; older results are discarded.
    pub(crate) file_generation: u64,
    /// Monotonic token for the newest scan; older results are discarded.
    pub(crate) scan_generation: u64,
    /// Monotonic token for the newest icon batch; older results are discarded.
    pub(crate) icon_generation: u64,
}

#[derive(Debug, Clone)]
pub enum Message {
    Ui(ui::Message),
    Settings(ui::SettingsMessage),
    SettingsOpened(window::Id),
    Platform,
    EnsureFocus,
    WindowReady(window::Id, isize),
    ScanComplete {
        generation: u64,
        scan: ScanResult,
    },
    IconsLoaded {
        generation: u64,
        icons: Vec<(String, Icon)>,
    },
    /// Result of a hotkey change that ran off the UI thread.
    HotkeyChangeFinished {
        choice: HotkeyChoice,
        status: HotkeyStatus,
    },
    /// Result of a `~` / `>` filesystem search.
    FileSearchComplete {
        generation: u64,
        files: Vec<FileEntry>,
    },
    Event(window::Id, Event),
}

impl Launcher {
    pub fn boot(settings: Settings) -> (Self, Task<Message>) {
        let theme = theme_from_choice(settings.theme);
        let mut launcher = Self {
            apps: Vec::new(),
            results: Vec::new(),
            calc: None,
            usage: Usage::load(),
            icons: HashMap::new(),
            files: Vec::new(),
            file_results: Vec::new(),
            file_query: None,
            settings,
            theme,
            query: String::new(),
            selected: 0,
            visible: platform::start_visible(),
            focused: false,
            window_id: None,
            settings_window: None,
            hwnd: None,
            hotkey: platform::hotkey_status(),
            hotkey_pending: None,
            settings_notice: None,
            error: None,
            warning: None,
            indexing: true,
            rescan_queued: false,
            file_generation: 0,
            scan_generation: 0,
            icon_generation: 0,
        };

        let (window_id, open) = window::open(launcher_window_settings(launcher.visible));
        launcher.window_id = Some(window_id);

        let ready = open.then(move |id| {
            window::raw_id::<Message>(id).map(move |raw| Message::WindowReady(id, raw as isize))
        });

        // Do not block window creation on the Start Menu scan. The window
        // appears immediately while the shell worker indexes in the background.
        let scan = launcher.begin_scan();
        let task = Task::batch([ready, scan]);

        (launcher, task)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Ui(message) => self.update_ui(message),
            Message::Settings(message) => self.update_settings(message),
            Message::SettingsOpened(id) => {
                self.settings_window = Some(id);
                Task::none()
            }
            Message::Platform => self.tick(),
            Message::EnsureFocus => self.ensure_focus(),
            Message::ScanComplete { generation, scan } => self.apply_scan(generation, scan),
            Message::IconsLoaded { generation, icons } => {
                if generation != self.icon_generation {
                    return Task::none();
                }

                for (key, icon) in icons {
                    self.icons
                        .insert(key, Handle::from_rgba(icon.width, icon.height, icon.rgba));
                }

                Task::none()
            }
            Message::HotkeyChangeFinished { choice, status } => {
                self.hotkey_pending = None;

                if status.available() {
                    self.settings.hotkey = choice;
                    self.hotkey = status;
                    self.settings_notice = None;
                    self.settings.save();
                } else {
                    // The platform restored the previous binding, so the stored
                    // choice and status stay as they were.
                    self.settings_notice = Some(format!(
                        "{} is already in use; select another hotkey.",
                        choice.label()
                    ));
                }

                Task::none()
            }
            Message::FileSearchComplete { generation, files } => {
                if generation != self.file_generation {
                    return Task::none();
                }

                self.files = files;
                self.recompute();

                Task::none()
            }
            Message::WindowReady(id, hwnd) => {
                self.window_id = Some(id);
                self.hwnd = Some(hwnd);
                util::log(format!("window ready (hwnd={hwnd:#x})"));

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
            Message::Event(window, event) => self.handle_event(window, event),
        }
    }

    fn update_ui(&mut self, message: ui::Message) -> Task<Message> {
        match message {
            ui::Message::QueryChanged(query) => self.change_query(query),
            ui::Message::ClearQuery => self.clear_query(),
            ui::Message::Navigate(delta) => {
                let total = self.selectable_count();

                if total > 0 {
                    let last = total - 1;
                    self.selected = if delta < 0 {
                        self.selected.saturating_sub(delta.unsigned_abs() as usize)
                    } else {
                        self.selected.saturating_add(delta as usize).min(last)
                    };
                }

                Task::none()
            }
            ui::Message::Activate(key) => self.activate(&key),
            ui::Message::ActivateFile(path) => self.open_path(&path),
            ui::Message::CopyCalc => self.copy_calc(),
            ui::Message::OpenSettings => self.open_settings(),
            ui::Message::Quit => iced::exit(),
            ui::Message::DragWindow => self
                .window_id
                .map(window::drag::<Message>)
                .unwrap_or_else(Task::none),
        }
    }

    fn change_query(&mut self, query: String) -> Task<Message> {
        self.query = query;
        self.calc = calc::evaluate(&self.query);
        self.file_query = file_index::parse_query(&self.query);
        self.error = None;

        if self.file_query.is_none() {
            self.files.clear();
            self.file_results.clear();
        }

        self.recompute();
        self.selected = 0;

        match &self.file_query {
            Some(file_query) if !file_query.is_empty() => {
                self.begin_file_search(file_query.term().to_owned())
            }
            Some(_) => {
                // A bare `~` or `>` shows the hint instead of walking the disk.
                self.files.clear();
                self.file_results.clear();
                Task::none()
            }
            None => Task::none(),
        }
    }

    fn clear_query(&mut self) -> Task<Message> {
        self.query.clear();
        self.calc = None;
        self.file_query = None;
        self.files.clear();
        self.file_results.clear();
        self.error = None;
        self.recompute();
        self.selected = 0;
        Task::none()
    }

    /// Starts a debounced filesystem walk; stale results are dropped.
    fn begin_file_search(&mut self, term: String) -> Task<Message> {
        self.file_generation = self.file_generation.wrapping_add(1);
        let generation = self.file_generation;

        Task::perform(
            async move {
                // A burst of keystrokes should walk the profile once, not once
                // per character.
                tokio::time::sleep(Duration::from_millis(140)).await;
                platform::search_files(term).await
            },
            move |files| Message::FileSearchComplete { generation, files },
        )
    }

    fn update_settings(&mut self, message: ui::SettingsMessage) -> Task<Message> {
        match message {
            ui::SettingsMessage::FontChanged(font) => {
                self.settings.font = font;
                self.settings.save();
                Task::none()
            }
            ui::SettingsMessage::AppNameFontChanged(font) => {
                self.settings.app_name_font = font;
                self.settings.save();
                Task::none()
            }
            ui::SettingsMessage::CategoryFontChanged(font) => {
                self.settings.category_font = font;
                self.settings.save();
                Task::none()
            }
            ui::SettingsMessage::ThemeChanged(theme) => {
                self.settings.theme = theme;
                self.theme = theme_from_choice(theme);
                self.settings.save();
                Task::none()
            }
            ui::SettingsMessage::HotkeyChanged(hotkey) => {
                if self.hotkey_pending.is_some() || hotkey == self.settings.hotkey {
                    return Task::none();
                }

                // Registering a hotkey round-trips through the platform's
                // message loop; run it as a task so the window keeps painting.
                self.hotkey_pending = Some(hotkey);
                self.settings_notice = None;

                Task::perform(platform::set_hotkey(hotkey), move |status| {
                    Message::HotkeyChangeFinished {
                        choice: hotkey,
                        status,
                    }
                })
            }
            ui::SettingsMessage::Close => self.close_settings(),
        }
    }

    fn close_settings(&mut self) -> Task<Message> {
        self.settings_window
            .take()
            .map(window::close::<Message>)
            .unwrap_or_else(Task::none)
    }

    fn open_settings(&mut self) -> Task<Message> {
        let task = if let Some(id) = self.settings_window {
            window::gain_focus::<Message>(id)
        } else {
            // The window id is recorded when the open task reports back, so a
            // failed open never leaves a dead id behind.
            let (_, open) = window::open(settings_window_settings());
            self.settings_notice = None;
            util::log("opening settings window");
            open.map(Message::SettingsOpened)
        };

        Task::batch([task, self.hide()])
    }

    pub fn view(&self, window: window::Id) -> Element<'_, Message> {
        if self.settings_window == Some(window) {
            ui::settings_view(&self.settings, &self.theme, self.settings_notice.as_deref())
                .map(Message::Settings)
        } else {
            ui::view(self.view_state()).map(Message::Ui)
        }
    }

    /// Borrows the read-only slice of state the launcher view renders.
    fn view_state(&self) -> ui::ViewState<'_> {
        let file_mode = self.file_query.is_some();

        ui::ViewState {
            apps: &self.apps,
            results: &self.results,
            calc: self.calc.as_ref(),
            icons: &self.icons,
            files: &self.files,
            file_results: &self.file_results,
            file_term: self.file_query.as_ref().map(|query| query.term()),
            query: &self.query,
            selected: self.selected,
            hotkey: self.hotkey,
            hotkey_choice: self.settings.hotkey,
            // The "no apps indexed" error is irrelevant while searching files.
            error: if file_mode {
                None
            } else {
                self.error.as_deref()
            },
            warning: self.warning.as_deref(),
            indexing: self.indexing,
            result_limit: self.result_limit(),
            theme: &self.theme,
            font: Font::with_name(self.settings.font.name()),
            app_name_font: Font::with_name(self.settings.app_name_font.name()),
            category_font: Font::with_name(self.settings.category_font.name()),
        }
    }

    pub fn title(&self, window: window::Id) -> String {
        if self.settings_window == Some(window) {
            "Pirsa Settings".to_owned()
        } else {
            "Pirsa".to_owned()
        }
    }

    pub fn window_theme(&self) -> Theme {
        self.theme.clone()
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
            iced::event::listen_with(|event, _status, window| match event {
                Event::Keyboard(_) | Event::Window(_) => Some(Message::Event(window, event)),
                _ => None,
            }),
        ])
    }

    fn tick(&mut self) -> Task<Message> {
        let requests = platform::take_requests();
        self.apply_requests(requests)
    }

    /// Handles every request collected since the last tick.
    ///
    /// Reading the flags clears them, so dropping one here loses it. Visibility
    /// requests are merged (a toggle already decides show or hide), while a
    /// settings request is independent of them.
    fn apply_requests(&mut self, requests: platform::Requests) -> Task<Message> {
        if requests.quit {
            return iced::exit();
        }

        let mut tasks = Vec::new();

        if requests.settings {
            tasks.push(self.open_settings());
        }

        if requests.toggle {
            tasks.push(self.toggle());
        } else if requests.show {
            tasks.push(self.show());
        }

        Task::batch(tasks)
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

        util::log(format!(
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

    /// Launches the app identified by its stable search key.
    ///
    /// The key survives rescans, so a click that races a scan can never launch
    /// whichever app happened to move into the same row.
    fn activate(&mut self, key: &str) -> Task<Message> {
        let Some(app) = self.apps.iter().find(|app| app.search_key() == key) else {
            return Task::none();
        };

        let name = app.name().to_owned();

        match platform::launch(app) {
            Ok(()) => {
                // Remember the launch before hiding so the next visit already
                // shows this app near the top of the list.
                self.usage.record(key);
                self.usage.save();
                self.hide()
            }
            Err(error) => {
                self.error = Some(format!("Could not launch {name}: {error}"));
                util::log(format!("launch failed for {name}: {error}"));
                Task::none()
            }
        }
    }

    fn activate_selected(&mut self) -> Task<Message> {
        if self.file_query.is_some() {
            return self.open_file(self.selected);
        }

        let position = if self.calc.is_some() {
            if self.selected == 0 {
                return self.copy_calc();
            }

            self.selected - 1
        } else {
            self.selected
        };

        self.activate_position(position)
    }

    /// Launches the app row at `position` in the result list.
    fn activate_position(&mut self, position: usize) -> Task<Message> {
        let Some(&index) = self.results.get(position) else {
            return Task::none();
        };

        let Some(app) = self.apps.get(index) else {
            return Task::none();
        };

        let key = app.search_key().to_owned();
        self.activate(&key)
    }

    /// `Ctrl+1` … `Ctrl+8` launch the nth row visible to the user.
    fn activate_number(&mut self, number: usize) -> Task<Message> {
        let position = number.saturating_sub(1);

        if self.file_query.is_some() {
            return self.open_file(position);
        }

        // App rows start after the calculation card when one is visible.
        self.activate_position(position + usize::from(self.calc.is_some()))
    }

    /// Opens the file result at `position`.
    fn open_file(&mut self, position: usize) -> Task<Message> {
        let Some(&index) = self.file_results.get(position) else {
            return Task::none();
        };

        let Some(file) = self.files.get(index) else {
            return Task::none();
        };

        let path = file.path().to_path_buf();
        self.open_path(&path)
    }

    fn open_path(&mut self, path: &std::path::Path) -> Task<Message> {
        if let Err(error) = platform::open_path(path) {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());

            self.error = Some(format!("Could not open {name}: {error}"));
            util::log(format!("open failed for {}: {error}", path.display()));

            return Task::none();
        }

        self.hide()
    }

    fn copy_calc(&mut self) -> Task<Message> {
        let Some(calc) = &self.calc else {
            return Task::none();
        };

        let text = calc.copy_text.clone();
        util::log("copied calculation result");

        Task::batch([iced::clipboard::write::<Message>(text), self.hide()])
    }

    /// Starts an asynchronous scan and tags it with a new generation.
    fn begin_scan(&mut self) -> Task<Message> {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        let generation = self.scan_generation;
        self.indexing = true;
        self.rescan_queued = false;

        Task::perform(platform::scan(), move |scan| Message::ScanComplete {
            generation,
            scan,
        })
    }

    fn rescan(&mut self) -> Task<Message> {
        // The scan worker serializes scans, so a request that arrives mid-scan
        // is queued and starts as soon as the current one lands.
        if self.indexing {
            self.rescan_queued = true;
            return Task::none();
        }

        self.begin_scan()
    }

    /// Replaces the indexed apps and refreshes the visible results. Shared by
    /// the startup scan and F5 so both follow the same rules.
    fn apply_scan(&mut self, generation: u64, scan: ScanResult) -> Task<Message> {
        if generation != self.scan_generation {
            if !cfg!(test) {
                util::log(format!(
                    "ignoring stale scan {generation} (current {})",
                    self.scan_generation
                ));
            }

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

        // Icons of apps that dropped out of the index must not linger, and the
        // batch below only covers the apps that still lack one.
        {
            let current: HashSet<&str> = self.apps.iter().map(|app| app.search_key()).collect();
            self.icons.retain(|key, _| current.contains(key.as_str()));
        }

        util::log(format!("indexed {count} apps"));

        let icons = self.load_icons();

        if self.rescan_queued {
            self.rescan_queued = false;
            return Task::batch([icons, self.begin_scan()]);
        }

        icons
    }

    /// Clears the query and any error, then rebuilds the result list.
    fn reset_search(&mut self) {
        self.query.clear();
        self.calc = None;
        self.error = None;
        self.recompute();
        self.selected = 0;
    }

    /// Loads icons for the apps that do not have one yet.
    fn load_icons(&mut self) -> Task<Message> {
        // Bumped even when nothing is missing so a late batch from a previous
        // scan cannot insert icons for apps that just disappeared.
        self.icon_generation = self.icon_generation.wrapping_add(1);
        let generation = self.icon_generation;

        let missing: Vec<AppEntry> = self
            .apps
            .iter()
            .filter(|app| !self.icons.contains_key(app.search_key()))
            .cloned()
            .collect();

        if missing.is_empty() {
            return Task::none();
        }

        Task::perform(platform::extract_icons(missing), move |icons| {
            Message::IconsLoaded { generation, icons }
        })
    }

    fn recompute(&mut self) {
        if let Some(file_query) = &self.file_query {
            self.file_results = file_index::rank(&self.files, file_query.term());
            self.results.clear();
        } else {
            self.results = search::rank_with_usage(&self.apps, &self.query, &self.usage);
            self.file_results.clear();
        }

        let selectable = self.selectable_count();

        if self.selected >= selectable {
            self.selected = selectable.saturating_sub(1);
        }
    }

    /// Number of rows that fit on screen in the current mode.
    pub(crate) fn result_limit(&self) -> usize {
        if self.file_query.is_some() {
            ui::MAX_RESULTS
        } else if self.calc.is_some() {
            ui::MAX_RESULTS_WITH_CALC
        } else {
            ui::MAX_RESULTS
        }
    }

    /// Calculation card first, then the visible app or file rows.
    fn selectable_count(&self) -> usize {
        if self.file_query.is_some() {
            return self.file_results.len().min(self.result_limit());
        }

        self.results.len().min(self.result_limit()) + usize::from(self.calc.is_some())
    }

    fn handle_event(&mut self, window_id: window::Id, event: Event) -> Task<Message> {
        if self.settings_window == Some(window_id) {
            return match event {
                Event::Window(window::Event::Closed) => {
                    self.settings_window = None;
                    util::log("settings window closed");
                    Task::none()
                }
                Event::Keyboard(KeyboardEvent::KeyPressed {
                    key: Key::Named(Named::Escape),
                    ..
                }) => self.close_settings(),
                _ => Task::none(),
            };
        }

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

        // Repeats only ever repeat navigation. Handling this first keeps a held
        // Ctrl+, from opening one settings window per repeat.
        if repeat {
            return match key {
                Key::Named(Named::ArrowUp) => self.update(Message::Ui(ui::Message::Navigate(-1))),
                Key::Named(Named::ArrowDown) => self.update(Message::Ui(ui::Message::Navigate(1))),
                _ => Task::none(),
            };
        }

        if modifiers.control() && matches!(key, Key::Character("q") | Key::Character("Q")) {
            return iced::exit();
        }

        if modifiers.control() && matches!(key, Key::Character(",")) {
            return self.open_settings();
        }

        // Ctrl+1 … Ctrl+8 launch the nth row.
        if modifiers.control()
            && let Key::Character(character) = &key
            && let Some(digit) = character
                .chars()
                .next()
                .and_then(|value| value.to_digit(10))
            && (1..=8).contains(&digit)
        {
            return self.activate_number(digit as usize);
        }

        match key {
            Key::Named(Named::Tab) => self.open_settings(),
            Key::Named(Named::Escape) => self.hide(),
            Key::Named(Named::ArrowUp) => self.update(Message::Ui(ui::Message::Navigate(-1))),
            Key::Named(Named::ArrowDown) => self.update(Message::Ui(ui::Message::Navigate(1))),
            Key::Named(Named::Enter) => self.activate_selected(),
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

fn launcher_window_settings(visible: bool) -> window::Settings {
    let platform_specific = window::settings::PlatformSpecific {
        skip_taskbar: true,
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::RoundSmall,
        ..window::settings::PlatformSpecific::default()
    };

    window::Settings {
        size: iced::Size::new(ui::WINDOW_WIDTH, ui::WINDOW_HEIGHT),
        position: window::Position::Centered,
        resizable: false,
        minimizable: false,
        closeable: false,
        decorations: false,
        transparent: false,
        visible,
        icon: app_icon(),
        exit_on_close_request: false,
        platform_specific,
        ..window::Settings::default()
    }
}

fn settings_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(ui::SETTINGS_WINDOW_WIDTH, ui::SETTINGS_WINDOW_HEIGHT),
        position: window::Position::Centered,
        resizable: false,
        minimizable: false,
        closeable: true,
        decorations: true,
        transparent: false,
        visible: true,
        icon: app_icon(),
        exit_on_close_request: true,
        ..window::Settings::default()
    }
}

fn app_icon() -> Option<window::icon::Icon> {
    window::icon::from_rgba(include_bytes!("../assets/icon-64.rgba").to_vec(), 64, 64).ok()
}

fn theme_from_choice(choice: ThemeChoice) -> Theme {
    match choice {
        ThemeChoice::Light => Theme::Light,
        ThemeChoice::Dark => Theme::custom(
            "Pirsa Dark",
            iced::theme::Palette {
                background: iced::Color::from_rgb8(0x15, 0x18, 0x20),
                text: iced::Color::from_rgb8(0xf3, 0xf5, 0xf9),
                primary: iced::Color::from_rgb8(0x7a, 0xa2, 0xf7),
                success: iced::Color::from_rgb8(0x73, 0xd8, 0xa3),
                warning: iced::Color::from_rgb8(0xe0, 0xaf, 0x68),
                danger: iced::Color::from_rgb8(0xf0, 0x7a, 0x8c),
            },
        ),
        ThemeChoice::Nord => Theme::Nord,
        ThemeChoice::Dracula => Theme::Dracula,
        ThemeChoice::GruvboxDark => Theme::GruvboxDark,
        ThemeChoice::CatppuccinMocha => Theme::CatppuccinMocha,
        ThemeChoice::TokyoNight => Theme::TokyoNight,
        ThemeChoice::SolarizedLight => Theme::SolarizedLight,
        ThemeChoice::GruvboxLight => Theme::GruvboxLight,
        ThemeChoice::CatppuccinLatte => Theme::CatppuccinLatte,
        ThemeChoice::TokyoNightLight => Theme::TokyoNightLight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pirsa_core::LaunchTarget;

    fn launcher_with_results(count: usize) -> Launcher {
        Launcher {
            apps: Vec::new(),
            results: (0..count).collect(),
            calc: None,
            usage: Usage::default(),
            icons: HashMap::new(),
            files: Vec::new(),
            file_results: Vec::new(),
            file_query: None,
            settings: Settings::default(),
            theme: theme_from_choice(ThemeChoice::Dark),
            query: String::new(),
            selected: 0,
            visible: true,
            focused: false,
            window_id: None,
            settings_window: None,
            hwnd: None,
            hotkey: HotkeyStatus::Pending,
            hotkey_pending: None,
            settings_notice: None,
            error: None,
            warning: None,
            indexing: false,
            rescan_queued: false,
            file_generation: 0,
            scan_generation: 0,
            icon_generation: 0,
        }
    }

    #[test]
    fn navigation_stays_within_visible_rows() {
        let mut launcher = launcher_with_results(20);

        let _ = launcher.update(Message::Ui(ui::Message::Navigate(100)));
        assert_eq!(launcher.selected, ui::MAX_RESULTS - 1);

        let _ = launcher.update(Message::Ui(ui::Message::Navigate(-100)));
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
    fn rescan_is_queued_while_indexing() {
        let mut launcher = launcher_with_results(0);
        launcher.scan_generation = 1;
        launcher.indexing = true;

        let _ = launcher.rescan();

        assert_eq!(launcher.scan_generation, 1);
        assert!(launcher.rescan_queued);

        launcher.indexing = false;
        let _ = launcher.rescan();

        assert_eq!(launcher.scan_generation, 2);
        assert!(!launcher.rescan_queued);
    }

    #[test]
    fn show_request_reveals_a_hidden_launcher() {
        let mut launcher = launcher_with_results(0);
        launcher.visible = false;

        let _ = launcher.apply_requests(platform::Requests {
            show: true,
            ..platform::Requests::default()
        });

        assert!(launcher.visible);
    }

    #[test]
    fn rejected_hotkey_keeps_the_previous_choice() {
        let mut launcher = launcher_with_results(0);
        launcher.settings.hotkey = HotkeyChoice::AltSpace;

        let _ = launcher.update(Message::HotkeyChangeFinished {
            choice: HotkeyChoice::CtrlSpace,
            status: HotkeyStatus::Unavailable,
        });

        assert_eq!(launcher.settings.hotkey, HotkeyChoice::AltSpace);
        assert!(launcher.settings_notice.is_some());
        assert_eq!(launcher.hotkey_pending, None);
    }

    #[test]
    fn accepted_hotkey_updates_the_choice() {
        let mut launcher = launcher_with_results(0);
        launcher.settings.hotkey = HotkeyChoice::AltSpace;

        let _ = launcher.update(Message::HotkeyChangeFinished {
            choice: HotkeyChoice::CtrlSpace,
            status: HotkeyStatus::Ready,
        });

        assert_eq!(launcher.settings.hotkey, HotkeyChoice::CtrlSpace);
        assert!(launcher.settings_notice.is_none());
    }

    #[test]
    fn icons_of_removed_apps_are_pruned() {
        let mut launcher = launcher_with_results(0);
        launcher.scan_generation = 1;
        launcher
            .icons
            .insert("gone".to_owned(), Handle::from_rgba(1, 1, vec![0; 4]));

        let _ = launcher.apply_scan(
            1,
            ScanResult {
                apps: vec![AppEntry::new(
                    "Notepad".to_owned(),
                    "Start Menu".to_owned(),
                    LaunchTarget::Shell("test:notepad".to_owned()),
                )],
                warning: None,
            },
        );

        assert!(!launcher.icons.contains_key("gone"));
    }

    #[test]
    fn calculation_card_is_the_first_selection() {
        let mut launcher = launcher_with_results(10);
        launcher.calc = Some(CalcResult {
            kind: calc::CalcKind::Math,
            expression: "2 + 2".to_owned(),
            primary: "4".to_owned(),
            detail: String::new(),
            copy_text: "4".to_owned(),
        });

        assert_eq!(launcher.result_limit(), ui::MAX_RESULTS_WITH_CALC);
        assert_eq!(launcher.selectable_count(), ui::MAX_RESULTS_WITH_CALC + 1);

        let _ = launcher.update(Message::Ui(ui::Message::Navigate(100)));
        assert_eq!(launcher.selected, ui::MAX_RESULTS_WITH_CALC);

        let _ = launcher.update(Message::Ui(ui::Message::Navigate(-100)));
        assert_eq!(launcher.selected, 0);
    }

    fn file_entry(path: &str, is_dir: bool) -> FileEntry {
        let path = std::path::PathBuf::from(path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        FileEntry::new(path, name, is_dir)
    }

    #[test]
    fn tilde_query_enters_file_mode() {
        let mut launcher = launcher_with_results(3);

        let _ = launcher.change_query("~report".to_owned());

        assert!(launcher.file_query.is_some());
        assert!(launcher.results.is_empty());
        assert_eq!(launcher.result_limit(), ui::MAX_RESULTS);
    }

    #[test]
    fn a_plain_query_leaves_file_mode() {
        let mut launcher = launcher_with_results(0);

        let _ = launcher.change_query("~report".to_owned());
        launcher
            .files
            .push(file_entry("C:\\Users\\me\\report.txt", false));

        let _ = launcher.change_query("note".to_owned());

        assert!(launcher.file_query.is_none());
        assert!(launcher.files.is_empty());
        assert!(launcher.file_results.is_empty());
    }

    #[test]
    fn file_results_rank_and_count_toward_selection() {
        let mut launcher = launcher_with_results(0);

        let _ = launcher.change_query("~report".to_owned());
        let generation = launcher.file_generation;

        let _ = launcher.update(Message::FileSearchComplete {
            generation,
            files: vec![
                file_entry("C:\\Users\\me\\old-report.txt", false),
                file_entry("C:\\Users\\me\\notes.txt", false),
                file_entry("C:\\Users\\me\\report.txt", false),
            ],
        });

        // "report.txt" (prefix match) outranks "old-report.txt" (substring).
        assert_eq!(launcher.file_results, vec![2, 0]);
        assert_eq!(launcher.selectable_count(), 2);

        let _ = launcher.update(Message::Ui(ui::Message::Navigate(100)));
        assert_eq!(launcher.selected, 1);
    }

    #[test]
    fn stale_file_searches_are_ignored() {
        let mut launcher = launcher_with_results(0);

        let _ = launcher.change_query("~report".to_owned());
        let stale = launcher.file_generation;

        let _ = launcher.change_query("~report2".to_owned());
        let _ = launcher.update(Message::FileSearchComplete {
            generation: stale,
            files: vec![file_entry("C:\\Users\\me\\report.txt", false)],
        });

        assert!(launcher.files.is_empty());
    }

    #[test]
    fn a_bare_prefix_does_not_search() {
        let mut launcher = launcher_with_results(0);

        let _ = launcher.change_query("~".to_owned());

        assert!(
            launcher
                .file_query
                .as_ref()
                .is_some_and(|query| query.is_empty())
        );
        assert!(launcher.files.is_empty());
        assert_eq!(launcher.selectable_count(), 0);
    }
}
