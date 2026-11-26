//! Launcher state and iced update logic.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use iced::futures::{StreamExt, stream};
use iced::keyboard::key::Named;
use iced::keyboard::{Event as KeyboardEvent, Key, Modifiers};
use iced::widget::image::Handle;
use iced::widget::operation;
use iced::{Event, Subscription, Task, Theme, window};

use pirsa_core::calc::{self, CalcResult};
use pirsa_core::folders::{self, FolderEntry};
use pirsa_core::results;
use pirsa_core::results::ResultRow;
use pirsa_core::search;
use pirsa_core::search_scope::{DriveDetails, DriveMode, IndexStatus, SearchScope};
use pirsa_core::settings::{HotkeyChoice, Settings, ThemeChoice};
use pirsa_core::usage::Usage;
use pirsa_core::util;
use pirsa_core::{AppEntry, HotkeyStatus, Icon, ScanResult};
use pirsa_platform as platform;
use pirsa_ui as ui;

/// The application's complete state.
pub struct Launcher {
    pub(crate) apps: Vec<AppEntry>,
    /// Apps and folders ranked into one list for the current query.
    pub(crate) rows: Vec<ResultRow>,
    /// Optional calculator/unit-conversion result for the current query.
    pub(crate) calc: Option<CalcResult>,
    /// Launch history: how often each app was opened, used for ordering.
    pub(crate) usage: Usage,
    /// Real application icons, keyed by display name.
    pub(crate) icons: HashMap<String, Handle>,
    /// Folder matches of the current query, indexed by [`ResultRow::Folder`].
    pub(crate) folders: Vec<FolderEntry>,
    /// Where folder search looks and what it skips.
    pub(crate) scope: SearchScope,
    pub(crate) drives: Vec<DriveDetails>,
    pub(crate) exclude_draft: String,
    pub(crate) index_status: IndexStatus,
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
    /// Category shown in the settings window's right column.
    pub(crate) settings_category: ui::SettingsCategory,
    pub(crate) error: Option<String>,
    pub(crate) warning: Option<String>,
    pub(crate) indexing: bool,
    /// Set when F5 arrives while a scan is running; runs once the scan lands.
    pub(crate) rescan_queued: bool,
    /// Monotonic token for the newest folder search; older results are discarded.
    pub(crate) folder_generation: u64,
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
    /// Result of a folder search for the current query.
    FolderSearchComplete {
        generation: u64,
        folders: Vec<FolderEntry>,
    },
    /// Result of the native folder picker.
    FolderPicked(Option<PathBuf>),
    /// Periodic refresh while the folder index is building.
    IndexTick,
    Event(window::Id, Event),
}

impl Launcher {
    pub fn boot(settings: Settings) -> (Self, Task<Message>) {
        let theme = theme_from_choice(settings.theme);
        let scope = SearchScope::load();
        let drives = platform::drives::list();
        platform::ensure_folder_index(&scope);
        let index_status = platform::folder_index_status();
        let mut launcher = Self {
            apps: Vec::new(),
            rows: Vec::new(),
            calc: None,
            usage: Usage::load(),
            icons: HashMap::new(),
            folders: Vec::new(),
            scope,
            drives,
            exclude_draft: String::new(),
            index_status,
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
            settings_category: ui::SettingsCategory::default(),
            error: None,
            warning: None,
            indexing: true,
            rescan_queued: false,
            folder_generation: 0,
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
        let ticks = launcher.start_index_tick();
        let task = Task::batch([ready, scan, ticks]);

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
            Message::FolderSearchComplete {
                generation,
                folders,
            } => {
                if generation != self.folder_generation {
                    return Task::none();
                }

                self.folders = folders;
                self.recompute();

                Task::none()
            }
            Message::FolderPicked(picked) => match picked {
                Some(path) => {
                    self.scope.add_include(path);
                    self.scope_changed()
                }
                None => Task::none(),
            },
            Message::IndexTick => {
                self.index_status = platform::folder_index_status();
                self.start_index_tick()
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
            ui::Message::ActivateFolder(path) => self.open_folder_path(&path),
            ui::Message::CopyCalc => self.copy_calc(),
            ui::Message::OpenSettings => self.open_settings(),
            ui::Message::Quit => iced::exit(),
            ui::Message::DragWindow => self
                .window_id
                .map_or_else(Task::none, window::drag::<Message>),
        }
    }

    fn change_query(&mut self, query: String) -> Task<Message> {
        self.query = query;
        self.calc = calc::evaluate(&self.query);
        self.error = None;

        let term = self.query.trim().to_owned();

        if term.is_empty() {
            // Nothing to match, so drop the previous folder matches instead of
            // letting stale rows linger behind an empty query.
            self.folders.clear();
        }

        // Matches still on hand are re-ranked against the new query right away;
        // folders that no longer match drop out on their own.
        self.recompute();
        self.selected = 0;

        if term.is_empty() {
            Task::none()
        } else {
            self.begin_folder_search(term)
        }
    }

    fn clear_query(&mut self) -> Task<Message> {
        self.query.clear();
        self.calc = None;
        self.folders.clear();
        self.error = None;
        self.recompute();
        self.selected = 0;
        Task::none()
    }

    /// Starts a debounced filesystem walk; stale results are dropped.
    fn begin_folder_search(&mut self, term: String) -> Task<Message> {
        self.folder_generation = self.folder_generation.wrapping_add(1);
        let generation = self.folder_generation;
        let scope = self.scope.clone();

        Task::perform(
            async move {
                // A burst of keystrokes should walk the profile once, not once
                // per character.
                tokio::time::sleep(Duration::from_millis(140)).await;
                platform::search_folders(scope, term).await
            },
            move |folders| Message::FolderSearchComplete {
                generation,
                folders,
            },
        )
    }

    fn start_index_tick(&self) -> Task<Message> {
        if !self.index_status.building {
            return Task::none();
        }

        Task::future(async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            Message::IndexTick
        })
    }

    /// Rebuilds the folder index from scratch.
    fn rebuild_index(&mut self) -> Task<Message> {
        if !cfg!(test) {
            platform::rebuild_folder_index(&self.scope);
        }

        self.index_status = platform::folder_index_status();
        self.start_index_tick()
    }

    /// Persists a scope edit and makes sure the index matches it.
    fn scope_changed(&mut self) -> Task<Message> {
        self.scope.save();

        if !cfg!(test) {
            platform::ensure_folder_index(&self.scope);
        }

        self.index_status = platform::folder_index_status();
        self.folders.clear();
        self.folder_generation = self.folder_generation.wrapping_add(1);
        // Rows index into the folder matches just dropped.
        self.recompute();

        self.start_index_tick()
    }

    /// Drive letters currently selected, defaulting to the attached drives.
    fn selected_letters(&self) -> Vec<char> {
        match &self.scope.drives {
            DriveMode::Selected(letters) => letters.clone(),
            _ => self.drives.iter().map(|drive| drive.info.letter).collect(),
        }
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
            ui::SettingsMessage::ScopePresetChanged(preset) => {
                self.scope.drives = match preset {
                    ui::ScopePreset::Profile => DriveMode::Profile,
                    ui::ScopePreset::Fixed => DriveMode::Fixed,
                    ui::ScopePreset::All => DriveMode::All,
                    ui::ScopePreset::Selected => {
                        let letters = self.selected_letters();

                        if letters.is_empty() {
                            DriveMode::Profile
                        } else {
                            DriveMode::Selected(letters)
                        }
                    }
                };

                self.scope_changed()
            }
            ui::SettingsMessage::DriveToggled(letter, checked) => {
                let mut letters = self.selected_letters();

                if checked {
                    if !letters.contains(&letter) {
                        letters.push(letter);
                    }
                } else {
                    letters.retain(|existing| *existing != letter);
                }

                letters.sort_unstable();

                self.scope.drives = if letters.is_empty() {
                    DriveMode::Profile
                } else {
                    DriveMode::Selected(letters)
                };

                self.scope_changed()
            }
            ui::SettingsMessage::AddFolderRequested => {
                Task::perform(platform::pick_folder(), Message::FolderPicked)
            }
            ui::SettingsMessage::IncludeRemoved(path) => {
                self.scope.remove_include(&path);
                self.scope_changed()
            }
            ui::SettingsMessage::ExcludeDraftChanged(draft) => {
                self.exclude_draft = draft;
                Task::none()
            }
            ui::SettingsMessage::ExcludeAdded => {
                if self.scope.add_exclude(&self.exclude_draft) {
                    self.exclude_draft.clear();
                    return self.scope_changed();
                }

                self.exclude_draft.clear();
                Task::none()
            }
            ui::SettingsMessage::ExcludeRemoved(rule) => {
                self.scope.remove_exclude(&rule);
                self.scope_changed()
            }
            ui::SettingsMessage::DepthChanged(depth) => {
                // Only stores the new depth; the rebuild waits for release so a
                // drag does not restart the indexer on every step.
                self.scope.set_depth(depth as usize);
                self.scope.save();
                Task::none()
            }
            ui::SettingsMessage::ScopeCommitted => self.scope_changed(),
            ui::SettingsMessage::HiddenToggled(hidden) => {
                self.scope.include_hidden = hidden;
                self.scope_changed()
            }
            ui::SettingsMessage::RebuildIndex => self.rebuild_index(),
            ui::SettingsMessage::DragWindow => self
                .settings_window
                .map_or_else(Task::none, window::drag::<Message>),
            ui::SettingsMessage::CategorySelected(category) => {
                self.settings_category = category;
                Task::none()
            }
        }
    }

    fn close_settings(&mut self) -> Task<Message> {
        self.settings_window
            .take()
            .map_or_else(Task::none, window::close::<Message>)
    }

    fn open_settings(&mut self) -> Task<Message> {
        // Drives can be plugged in after boot, so refresh before showing the
        // scope controls.
        self.drives = platform::drives::list();

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

    // Signature is fixed by `iced::daemon`, which passes the state.
    #[allow(clippy::unused_self)]
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
        let actually_visible = self.hwnd.map_or(self.visible, platform::window::is_visible);

        let foreground = self.hwnd.is_some_and(platform::window::is_foreground);

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
            let _ = platform::window::focus(hwnd);
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

        self.window_id.map_or_else(Task::none, |id| {
            window::set_mode::<Message>(id, window::Mode::Hidden)
        })
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
        if self.calc.is_some() && self.selected == 0 {
            return self.copy_calc();
        }

        self.activate_row(
            self.selected
                .saturating_sub(usize::from(self.calc.is_some())),
        )
    }

    /// Launches whatever row sits at `position`, app or folder.
    fn activate_row(&mut self, position: usize) -> Task<Message> {
        match self.rows.get(position) {
            Some(ResultRow::App(index)) => {
                let Some(app) = self.apps.get(*index) else {
                    return Task::none();
                };

                let key = app.search_key().to_owned();
                self.activate(&key)
            }
            Some(ResultRow::Folder(index)) => {
                let Some(folder) = self.folders.get(*index) else {
                    return Task::none();
                };

                let path = folder.path().to_path_buf();
                self.open_folder_path(&path)
            }
            None => Task::none(),
        }
    }

    /// `Ctrl+1` … `Ctrl+8` launch the nth row visible to the user.
    fn activate_number(&mut self, number: usize) -> Task<Message> {
        self.activate_row(number.saturating_sub(1))
    }

    fn open_folder_path(&mut self, path: &std::path::Path) -> Task<Message> {
        if let Err(error) = platform::open_folder(path) {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );

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
            let current: HashSet<&str> = self.apps.iter().map(AppEntry::search_key).collect();
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
        self.folders.clear();
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
        let apps = search::rank_scores(&self.apps, &self.query, &self.usage);
        let folders = folders::rank_scores(&self.folders, &self.query);

        self.rows = results::merge(&apps, &folders, usize::MAX);

        let selectable = self.selectable_count();

        if self.selected >= selectable {
            self.selected = selectable.saturating_sub(1);
        }
    }

    /// Number of rows that fit on screen.
    pub(crate) fn result_limit(&self) -> usize {
        if self.calc.is_some() {
            ui::MAX_RESULTS_WITH_CALC
        } else {
            ui::MAX_RESULTS
        }
    }

    /// Calculation card first, then the visible app and folder rows.
    fn selectable_count(&self) -> usize {
        self.rows.len().min(self.result_limit()) + usize::from(self.calc.is_some())
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

        if modifiers.control() && matches!(key, Key::Character("q" | "Q")) {
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
            Key::Named(Named::F5) => {
                let scan = self.rescan();
                let index = self.rebuild_index();

                Task::batch([scan, index])
            }
            _ => Task::none(),
        }
    }
}

mod view;

#[cfg(test)]
mod tests;

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
    // Borderless like the launcher: the view draws its own frame and header,
    // and the header is the drag handle.
    let platform_specific = window::settings::PlatformSpecific {
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::RoundSmall,
        ..window::settings::PlatformSpecific::default()
    };

    window::Settings {
        size: iced::Size::new(ui::SETTINGS_WINDOW_WIDTH, ui::SETTINGS_WINDOW_HEIGHT),
        position: window::Position::Centered,
        resizable: false,
        minimizable: false,
        closeable: true,
        decorations: false,
        transparent: false,
        visible: true,
        icon: app_icon(),
        exit_on_close_request: true,
        platform_specific,
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
