//! Headless view tests for the launcher and settings windows.

use super::*;
use pirsa_core::LaunchTarget;
use pirsa_core::search_scope::DriveKind;
use pirsa_core::settings::Settings;

fn empty_scope() -> &'static SearchScope {
    static SCOPE: OnceLock<SearchScope> = OnceLock::new();

    SCOPE.get_or_init(SearchScope::default)
}

fn notepad() -> AppEntry {
    AppEntry::new(
        "Notepad".to_owned(),
        "Start Menu".to_owned(),
        LaunchTarget::Shell("test:notepad".to_owned()),
    )
}

fn app_state<'a>(
    apps: &'a [AppEntry],
    rows: &'a [ResultRow],
    icons: &'a HashMap<String, Handle>,
    theme: &'a Theme,
) -> ViewState<'a> {
    ViewState {
        apps,
        rows,
        calc: None,
        icons,
        folders: &[],
        scope: empty_scope(),
        drives: &[],
        exclude_draft: "",
        index_status: IndexStatus::default(),
        query: "",
        selected: 0,
        hotkey: HotkeyStatus::Ready,
        hotkey_choice: HotkeyChoice::AltSpace,
        error: None,
        warning: None,
        indexing: false,
        result_limit: MAX_RESULTS,
        theme,
        font: Font::with_name("Segoe UI"),
        app_name_font: Font::with_name("Segoe UI"),
        category_font: Font::with_name("Segoe UI"),
    }
}

#[test]
fn launcher_rows_can_be_activated_headlessly() {
    let apps = [notepad()];
    let rows = [ResultRow::App(0)];
    let icons = HashMap::new();
    let theme = Theme::Dark;
    let mut ui = iced_tester::test::simulator(view(app_state(&apps, &rows, &icons, &theme)));

    assert!(ui.find("Notepad").is_ok());
    ui.click("Notepad")
        .expect("Notepad row should be clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::Activate(key) if key == "notepad")),
        "clicking the row should activate the app by its stable key"
    );
}

#[test]
fn settings_button_opens_settings_headlessly() {
    let apps = [notepad()];
    let rows = [ResultRow::App(0)];
    let icons = HashMap::new();
    let theme = Theme::Dark;
    let mut ui = iced_tester::test::simulator(view(app_state(&apps, &rows, &icons, &theme)));

    ui.click("Settings")
        .expect("Settings button should be clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::OpenSettings)),
        "clicking Settings should emit OpenSettings"
    );
}

#[test]
fn index_warning_is_shown_in_the_footer() {
    let apps = [notepad()];
    let rows = [ResultRow::App(0)];
    let icons = HashMap::new();
    let theme = Theme::Dark;
    let mut state = app_state(&apps, &rows, &icons, &theme);
    state.warning = Some("Windows apps could not be indexed.");

    let mut ui = iced_tester::test::simulator(view(state));

    assert!(ui.find("Windows apps could not be indexed.").is_ok());
}

#[test]
fn folder_rows_are_clickable_headlessly() {
    let apps = [];
    let rows = [ResultRow::Folder(0)];
    let icons = HashMap::new();
    let theme = Theme::Dark;
    let folders = [FolderEntry::new(
        PathBuf::from("C:\\Users\\me\\Documents\\reports"),
        "reports".to_owned(),
    )];

    let mut state = app_state(&apps, &rows, &icons, &theme);
    state.folders = &folders;
    state.query = ":report";

    let mut ui = iced_tester::test::simulator(view(state));

    assert!(ui.find("Folders").is_ok());
    assert!(ui.find("reports").is_ok());
    ui.click("reports")
        .expect("a folder row should be clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::ActivateFolder(path) if path.ends_with("reports")
        )),
        "clicking a folder row should emit ActivateFolder for that path"
    );
}

#[test]
fn folder_mode_uses_the_folders_label() {
    let apps = [notepad()];
    let rows = [ResultRow::Folder(0)];
    let icons = HashMap::new();
    let theme = Theme::Dark;
    let folders = [FolderEntry::new(
        PathBuf::from("C:\\Users\\me\\Documents\\notes"),
        "notes".to_owned(),
    )];

    let mut state = app_state(&apps, &rows, &icons, &theme);
    state.folders = &folders;
    state.query = ":note";

    let mut ui = iced_tester::test::simulator(view(state));

    assert!(
        ui.find("Folders").is_ok(),
        "folder mode has its own heading"
    );
    assert!(ui.find("1 FOUND").is_ok());
    assert!(ui.find("notes").is_ok(), "the folder row is listed");
    assert!(ui.find("Notepad").is_err(), "app rows are not shown");
    assert!(ui.find("All apps").is_err());
}

#[test]
fn a_bare_trigger_shows_the_folder_hint() {
    let apps = [];
    let rows = [];
    let icons = HashMap::new();
    let theme = Theme::Dark;
    let mut state = app_state(&apps, &rows, &icons, &theme);
    state.query = ":";

    let mut ui = iced_tester::test::simulator(view(state));

    assert!(ui.find("Type a folder name after “:”").is_ok());
}

fn settings_state<'a>(
    settings: &'a Settings,
    scope: &'a SearchScope,
    drives: &'a [DriveDetails],
    index_status: IndexStatus,
    theme: &'a Theme,
    notice: Option<&'a str>,
    category: SettingsCategory,
) -> SettingsView<'a> {
    SettingsView {
        settings,
        fonts: FontChoice::ALL,
        scope,
        drives,
        trigger_draft: &scope.trigger,
        exclude_draft: "",
        index_status,
        theme,
        notice,
        category,
    }
}

#[test]
fn emphasis_weights_preserve_installed_families() {
    use iced::font::{Family, Weight};

    let segoe = semibold_font(Font::with_name("Segoe UI"));
    assert_eq!(segoe.weight, Weight::Semibold);

    for family in ["Arial", "Consolas", "Georgia"] {
        let emphasized = semibold_font(Font::with_name(family));

        assert_eq!(
            emphasized.weight,
            Weight::Medium,
            "{family} needs a weight that keeps its family"
        );
        assert_eq!(emphasized.family, Family::Name(family));
    }

    // Segoe UI Variable is registered at 400/500 only.
    let variable = semibold_font(Font::with_name("Segoe UI Variable"));
    assert_eq!(variable.weight, Weight::Medium);
    assert_eq!(
        bold_font(Font::with_name("Segoe UI Variable")).weight,
        Weight::Medium
    );
}

#[test]
fn available_fonts_only_contain_real_choices() {
    let available = available_fonts();

    assert!(!available.is_empty(), "the system should expose some fonts");
    assert!(
        available.contains(&FontChoice::SegoeUi),
        "Segoe UI is part of Windows"
    );
    assert!(
        available
            .iter()
            .all(|choice| FontChoice::ALL.contains(choice)),
        "the picker must not invent font choices"
    );
}

#[test]
fn settings_view_builds_and_closes_headlessly() {
    let settings = Settings::default();
    let scope = SearchScope::default();
    let theme = Theme::Dark;
    let mut ui = iced_tester::test::simulator(settings_view(settings_state(
        &settings,
        &scope,
        &[],
        IndexStatus::default(),
        &theme,
        None,
        SettingsCategory::default(),
    )));

    // First column lists every category.
    for category in SettingsCategory::ALL {
        assert!(
            ui.find(category.label()).is_ok(),
            "the category column should list {}",
            category.label()
        );
    }

    // Second column shows only the selected category.
    assert!(ui.find("UI font").is_ok());
    assert!(ui.find("App name font").is_ok());
    assert!(ui.find("Category font").is_ok());
    assert!(ui.find("Theme").is_ok());
    assert!(ui.find("Global hotkey").is_err());
    assert!(ui.find("Scope").is_err());

    ui.click("Close").expect("Close button should be clickable");

    let messages: Vec<SettingsMessage> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, SettingsMessage::Close)),
        "clicking Close should emit SettingsMessage::Close"
    );
}

#[test]
fn settings_category_column_switches_the_detail_column() {
    let settings = Settings::default();
    let scope = SearchScope::default();
    let theme = Theme::Dark;
    let mut ui = iced_tester::test::simulator(settings_view(settings_state(
        &settings,
        &scope,
        &[],
        IndexStatus::default(),
        &theme,
        None,
        SettingsCategory::Appearance,
    )));

    ui.click("Folder search")
        .expect("Folder search should be a category button");

    let messages: Vec<SettingsMessage> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            SettingsMessage::CategorySelected(SettingsCategory::FolderSearch)
        )),
        "clicking a category should emit CategorySelected for it"
    );

    let mut ui = iced_tester::test::simulator(settings_view(settings_state(
        &settings,
        &scope,
        &[],
        IndexStatus::default(),
        &theme,
        None,
        SettingsCategory::FolderSearch,
    )));

    assert!(ui.find("Trigger").is_ok());
    assert!(ui.find("Scope").is_ok());
    assert!(ui.find("Search depth").is_ok());
    assert!(ui.find("UI font").is_err());
}

#[test]
fn settings_view_lists_drives_and_index_state() {
    let settings = Settings::default();
    let scope = SearchScope {
        drives: DriveMode::Selected(vec!['D']),
        ..SearchScope::default()
    };
    let drives = [
        DriveDetails {
            info: pirsa_core::search_scope::DriveInfo {
                letter: 'C',
                kind: DriveKind::Fixed,
            },
            label: "System".to_owned(),
            free_bytes: 10 * 1024 * 1024 * 1024,
            total_bytes: 100 * 1024 * 1024 * 1024,
        },
        DriveDetails {
            info: pirsa_core::search_scope::DriveInfo {
                letter: 'D',
                kind: DriveKind::Fixed,
            },
            label: "Work".to_owned(),
            free_bytes: 20 * 1024 * 1024 * 1024,
            total_bytes: 200 * 1024 * 1024 * 1024,
        },
    ];
    let theme = Theme::Dark;
    let status = IndexStatus {
        ready: true,
        building: false,
        folders: 1234,
    };

    let mut ui = iced_tester::test::simulator(settings_view(settings_state(
        &settings,
        &scope,
        &drives,
        status,
        &theme,
        None,
        SettingsCategory::FolderSearch,
    )));

    let system = drives[0].summary();
    let work = drives[1].summary();
    assert!(ui.find(system.as_str()).is_ok());
    assert!(ui.find(work.as_str()).is_ok());
    assert!(ui.find("1234 folders indexed").is_ok());
}

#[test]
fn settings_view_shows_hotkey_notice_headlessly() {
    let settings = Settings::default();
    let scope = SearchScope::default();
    let theme = Theme::Dark;
    let notice = "Ctrl+Space is already in use; select another hotkey.";
    let mut ui = iced_tester::test::simulator(settings_view(settings_state(
        &settings,
        &scope,
        &[],
        IndexStatus::default(),
        &theme,
        Some(notice),
        SettingsCategory::Behavior,
    )));

    assert!(ui.find(notice).is_ok());
    assert!(ui.find("Global hotkey").is_ok());
}

#[test]
fn settings_header_drags_the_borderless_window_headlessly() {
    let settings = Settings::default();
    let scope = SearchScope::default();
    let theme = Theme::Dark;
    let mut ui = iced_tester::test::simulator(settings_view(settings_state(
        &settings,
        &scope,
        &[],
        IndexStatus::default(),
        &theme,
        None,
        SettingsCategory::default(),
    )));

    ui.click("Changes apply immediately.")
        .expect("the header should be a drag handle");

    let messages: Vec<SettingsMessage> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, SettingsMessage::DragWindow)),
        "pressing the header should emit SettingsMessage::DragWindow"
    );
}
