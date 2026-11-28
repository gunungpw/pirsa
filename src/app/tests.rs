//! Headless tests for the launcher state machine.

use super::*;
use pirsa_core::LaunchTarget;

fn launcher_with_results(count: usize) -> Launcher {
    Launcher {
        apps: Vec::new(),
        rows: (0..count).map(ResultRow::App).collect(),
        calc: None,
        usage: Usage::default(),
        icons: HashMap::new(),
        folders: Vec::new(),
        scope: SearchScope::default(),
        drives: Vec::new(),
        trigger_draft: SearchScope::default().trigger.clone(),
        exclude_draft: String::new(),
        index_status: IndexStatus::default(),
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
        settings_category: ui::SettingsCategory::default(),
        error: None,
        warning: None,
        indexing: false,
        rescan_queued: false,
        folder_generation: 0,
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

fn folder_entry(path: &str) -> FolderEntry {
    let path = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    FolderEntry::new(path, name)
}

#[test]
fn plain_queries_search_apps_only() {
    let mut launcher = launcher_with_results(0);
    launcher.apps.push(AppEntry::new(
        "Reports".to_owned(),
        "Start Menu".to_owned(),
        LaunchTarget::Shell("test:reports".to_owned()),
    ));
    launcher
        .folders
        .push(folder_entry("C:\\Users\\me\\reports"));

    let _ = launcher.change_query("reports".to_owned());

    assert_eq!(launcher.rows, vec![ResultRow::App(0)]);
    assert!(
        launcher.folders.is_empty(),
        "folder search is off without the trigger"
    );
}

#[test]
fn the_trigger_searches_folders_only() {
    let mut launcher = launcher_with_results(0);
    launcher.apps.push(AppEntry::new(
        "Reports".to_owned(),
        "Start Menu".to_owned(),
        LaunchTarget::Shell("test:reports".to_owned()),
    ));
    launcher.folders = vec![
        folder_entry("C:\\Users\\me\\old-reports"),
        folder_entry("C:\\Users\\me\\notes"),
        folder_entry("C:\\Users\\me\\reports"),
    ];

    let _ = launcher.change_query(":reports".to_owned());

    // "reports" (prefix match) outranks "old-reports" (substring) and
    // "notes" does not match at all. Apps never appear in folder mode.
    assert_eq!(
        launcher.rows,
        vec![ResultRow::Folder(2), ResultRow::Folder(0)]
    );
    assert_eq!(launcher.selectable_count(), 2);

    let _ = launcher.update(Message::Ui(ui::Message::Navigate(100)));
    assert_eq!(launcher.selected, 1);
}

#[test]
fn folder_matches_that_stop_matching_drop_out() {
    let mut launcher = launcher_with_results(0);
    launcher
        .folders
        .push(folder_entry("C:\\Users\\me\\reports"));

    let _ = launcher.change_query(":report".to_owned());
    assert_eq!(launcher.rows, vec![ResultRow::Folder(0)]);

    let _ = launcher.change_query(":note".to_owned());

    assert!(launcher.rows.is_empty(), "the stale folder drops out");
    assert_eq!(
        launcher.folders.len(),
        1,
        "matches are kept until the new search lands"
    );
}

#[test]
fn an_empty_query_clears_folder_matches() {
    let mut launcher = launcher_with_results(0);
    launcher
        .folders
        .push(folder_entry("C:\\Users\\me\\reports"));

    let _ = launcher.change_query(":report".to_owned());
    let _ = launcher.change_query("   ".to_owned());

    assert_eq!(launcher.folders, [] as [FolderEntry; 0]);
    assert_eq!(launcher.rows, [] as [ResultRow; 0]);
    assert_eq!(launcher.selectable_count(), 0);
}

#[test]
fn stale_folder_searches_are_ignored() {
    let mut launcher = launcher_with_results(0);

    let _ = launcher.change_query(":report".to_owned());
    let stale = launcher.folder_generation;

    let _ = launcher.change_query(":report2".to_owned());
    let _ = launcher.update(Message::FolderSearchComplete {
        generation: stale,
        folders: vec![folder_entry("C:\\Users\\me\\reports")],
    });

    assert_eq!(launcher.folders, [] as [FolderEntry; 0]);
}

#[test]
fn a_custom_trigger_is_used() {
    let mut launcher = launcher_with_results(0);
    launcher.scope.trigger = ">>".to_owned();
    launcher
        .folders
        .push(folder_entry("C:\\Users\\me\\reports"));

    let _ = launcher.change_query(">>reports".to_owned());
    assert_eq!(launcher.rows, vec![ResultRow::Folder(0)]);

    let _ = launcher.change_query(":reports".to_owned());
    assert!(
        launcher.rows.is_empty(),
        ": is an app query once the trigger is >>"
    );
}

#[test]
fn a_bare_trigger_clears_folder_rows() {
    let mut launcher = launcher_with_results(0);
    launcher
        .folders
        .push(folder_entry("C:\\Users\\me\\reports"));

    let _ = launcher.change_query(":reports".to_owned());
    assert_eq!(launcher.rows, vec![ResultRow::Folder(0)]);

    let _ = launcher.change_query(":".to_owned());
    assert!(launcher.rows.is_empty());
    assert!(launcher.folders.is_empty());
}

#[test]
fn folder_mode_disables_the_calculator() {
    let mut launcher = launcher_with_results(0);

    let _ = launcher.change_query("1+1".to_owned());
    assert!(launcher.calc.is_some());

    let _ = launcher.change_query(":1+1".to_owned());
    assert!(launcher.calc.is_none());
    assert!(launcher.rows.is_empty());
}

#[test]
fn a_valid_trigger_is_committed() {
    let mut launcher = launcher_with_results(0);
    launcher.trigger_draft = "  >>  ".to_owned();

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::TriggerCommitted));

    assert_eq!(launcher.scope.trigger, ">>");
    assert_eq!(launcher.trigger_draft, ">>");
    assert!(launcher.settings_notice.is_none());
}

#[test]
fn an_invalid_trigger_is_rejected() {
    let mut launcher = launcher_with_results(0);
    launcher.scope.trigger = ":".to_owned();
    launcher.trigger_draft = "two words".to_owned();

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::TriggerCommitted));

    assert_eq!(launcher.scope.trigger, ":");
    assert!(launcher.settings_notice.is_some());
}

#[test]
fn toggling_a_drive_switches_to_selected_drives() {
    let mut launcher = launcher_with_results(0);

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::DriveToggled(
        'D', true,
    )));

    assert_eq!(launcher.scope.drives, DriveMode::Selected(vec!['D']));

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::DriveToggled(
        'D', false,
    )));

    assert_eq!(launcher.scope.drives, DriveMode::Profile);
}

#[test]
fn a_scope_change_clears_stale_folder_results() {
    let mut launcher = launcher_with_results(0);
    launcher
        .folders
        .push(folder_entry("C:\\Users\\me\\reports"));
    launcher.rows.push(ResultRow::Folder(0));

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::DepthChanged(9)));
    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ScopeCommitted));

    assert_eq!(launcher.folders, [] as [FolderEntry; 0]);
    assert!(launcher.rows.is_empty(), "rows index the dropped folders");
    assert_eq!(launcher.scope.depth, 9);
}

#[test]
fn exclusion_rules_are_added_and_removed() {
    let mut launcher = launcher_with_results(0);

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ExcludeDraftChanged(
        "scratch".to_owned(),
    )));
    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ExcludeAdded));

    assert!(launcher.scope.exclude.contains(&"scratch".to_owned()));
    assert_eq!(launcher.exclude_draft, "");

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ExcludeRemoved(
        "scratch".to_owned(),
    )));

    assert!(!launcher.scope.exclude.contains(&"scratch".to_owned()));
}
