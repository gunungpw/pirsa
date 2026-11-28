//! Launcher and settings window rendering.

use iced::{Element, Font, Theme, window};

use pirsa_ui as ui;

use super::*;

impl Launcher {
    pub fn view(&self, window: window::Id) -> Element<'_, Message> {
        if self.settings_window == Some(window) {
            ui::settings_view(ui::SettingsView {
                settings: &self.settings,
                fonts: available_fonts(),
                scope: &self.scope,
                drives: &self.drives,
                trigger_draft: &self.trigger_draft,
                exclude_draft: &self.exclude_draft,
                index_status: self.index_status,
                theme: &self.theme,
                notice: self.settings_notice.as_deref(),
                category: self.settings_category,
            })
            .map(Message::Settings)
        } else {
            ui::view(self.view_state()).map(Message::Ui)
        }
    }

    /// Borrows the read-only slice of state the launcher view renders.
    fn view_state(&self) -> ui::ViewState<'_> {
        ui::ViewState {
            apps: &self.apps,
            rows: &self.rows,
            calc: self.calc.as_ref(),
            icons: &self.icons,
            folders: &self.folders,
            scope: &self.scope,
            drives: &self.drives,
            exclude_draft: &self.exclude_draft,
            index_status: self.index_status,
            query: &self.query,
            selected: self.selected,
            hotkey: self.hotkey,
            hotkey_choice: self.settings.hotkey,
            error: self.error.as_deref(),
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
}
