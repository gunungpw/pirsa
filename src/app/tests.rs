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
        files: Vec::new(),
        file_index: None,
        index_fingerprint: SearchScope::default().fingerprint(),
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

#[test]
fn plain_queries_search_apps_only() {
    let mut launcher = launcher_with_results(0);
    launcher.apps.push(AppEntry::new(
        "Reports".to_owned(),
        "Start Menu".to_owned(),
        LaunchTarget::Shell("test:reports".to_owned()),
    ));
    launcher
        .files
        .push(file_entry("C:\\Users\\me\\reports", false));

    let _ = launcher.change_query("reports".to_owned());

    assert_eq!(launcher.rows, vec![ResultRow::App(0)]);
    assert!(
        launcher.files.is_empty(),
        "file search is off without the trigger"
    );
}

fn file_entry(path: &str, is_dir: bool) -> FileEntry {
    let path = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key = path.to_string_lossy().to_lowercase();

    FileEntry::new(path, name, is_dir, 0, 0, format!("path:{key}"))
}

#[test]
fn trigger_results_render_files_and_folders() {
    let mut launcher = launcher_with_results(0);

    let _ = launcher.change_query(":reports".to_owned());
    launcher.apply_file_hits(vec![
        file_entry("C:\\Users\\me\\reports", true),
        file_entry("C:\\Users\\me\\annual-reports.pdf", false),
        file_entry("C:\\Users\\me\\report-2025.xlsx", false),
    ]);

    assert_eq!(
        launcher.rows,
        vec![ResultRow::File(0), ResultRow::File(1), ResultRow::File(2)]
    );
    assert_eq!(launcher.selectable_count(), 3);

    let _ = launcher.update(Message::Ui(ui::Message::Navigate(100)));
    assert_eq!(launcher.selected, 2);
}

#[test]
fn a_plain_query_clears_file_results() {
    let mut launcher = launcher_with_results(0);
    let _ = launcher.change_query(":report".to_owned());
    launcher.apply_file_hits(vec![file_entry("C:\\Users\\me\\reports", false)]);

    assert_eq!(launcher.rows, vec![ResultRow::File(0)]);

    let _ = launcher.change_query("report".to_owned());

    assert!(launcher.files.is_empty());
    assert!(launcher.rows.is_empty());
}

#[test]
fn stale_file_searches_are_ignored() {
    let mut launcher = launcher_with_results(0);
    launcher.file_generation = 7;

    let _ = launcher.update(Message::FileSearchComplete {
        generation: 6,
        files: vec![file_entry("C:\\Users\\me\\reports", false)],
    });

    assert!(launcher.files.is_empty());
    assert!(launcher.rows.is_empty());
}

#[test]
fn a_custom_trigger_is_used() {
    let mut launcher = launcher_with_results(0);
    launcher.scope.trigger = ">>".to_owned();

    let _ = launcher.change_query(">>reports".to_owned());
    launcher.apply_file_hits(vec![file_entry("C:\\Users\\me\\reports", true)]);
    assert_eq!(launcher.rows, vec![ResultRow::File(0)]);

    let _ = launcher.change_query(":reports".to_owned());
    assert!(launcher.rows.is_empty());
}

#[test]
fn a_bare_trigger_clears_file_rows() {
    let mut launcher = launcher_with_results(0);

    let _ = launcher.change_query(":reports".to_owned());
    launcher.apply_file_hits(vec![file_entry("C:\\Users\\me\\reports", false)]);
    assert_eq!(launcher.rows, vec![ResultRow::File(0)]);

    let _ = launcher.change_query(":".to_owned());
    assert!(launcher.rows.is_empty());
    assert!(launcher.files.is_empty());
}

#[test]
fn file_mode_disables_the_calculator() {
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
fn a_scope_change_clears_stale_file_results() {
    let mut launcher = launcher_with_results(0);
    launcher
        .files
        .push(file_entry("C:\\Users\\me\\reports", false));
    launcher.rows.push(ResultRow::File(0));

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::DepthChanged(9)));
    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ScopeCommitted));

    assert!(launcher.files.is_empty());
    assert!(launcher.rows.is_empty(), "rows index the dropped files");
    assert_eq!(launcher.scope.depth, 9);
}

#[test]
fn a_scope_change_asks_for_a_restart_until_reverted() {
    let mut launcher = launcher_with_results(0);
    let original = launcher.scope.depth;

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::DepthChanged(9)));
    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ScopeCommitted));

    assert!(launcher.scope_dirty());
    assert!(
        launcher
            .settings_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("Restart")),
        "the settings window must say the running index still uses the old scope"
    );

    let _ = launcher.update(Message::Settings(ui::SettingsMessage::DepthChanged(
        original as u8,
    )));
    let _ = launcher.update(Message::Settings(ui::SettingsMessage::ScopeCommitted));

    assert!(!launcher.scope_dirty());
    assert!(launcher.settings_notice.is_none());
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
