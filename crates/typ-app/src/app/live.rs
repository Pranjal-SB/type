//! A setting changed in the settings tab, applied to the running editor.
//!
//! The tab writes `config.toml` and says which setting it changed; it never
//! reaches the app (invariant 5). The app reads that off the tab after its
//! events, the way it reads a buffer off an editor, and applies the one
//! setting where it can. The rest wait for the next start, and the tab said so
//! before the change.

use super::App;
use crate::config::Settings;
use crate::keybindings::KeysView;
use crate::settings::SettingsView;

impl App {
    /// The colour depth themes are degraded to: what `main` detected or was
    /// told. A theme loaded while running has to come down to the same depth
    /// as the one loaded at start.
    pub fn set_color_depth(&mut self, depth: typ_core::Depth) {
        self.depth = depth;
    }

    /// `session` from `config.toml`: whether quitting writes the session.
    pub fn set_session_enabled(&mut self, enabled: bool) {
        self.session_enabled = enabled;
    }

    /// Apply whatever the settings tab on screen changed since it was last
    /// asked. Called after every batch of panel events.
    pub(super) fn apply_settings_change(&mut self) {
        let panel = self.panes[self.pane].shown_mut().panel_mut();
        let Some(view) = panel.as_any_mut().downcast_mut::<SettingsView>() else {
            return;
        };
        let dir = view.path().parent().map(std::path::Path::to_path_buf);
        let Some((key, settings)) = view.take_changed() else {
            return;
        };
        self.apply_setting(key, &settings, dir.as_deref());
    }

    /// Take the keymap the keybindings tab on screen wrote, if it wrote one.
    /// One keymap, so the `ctrl+k` menu, the hint and the palette, which all
    /// read it, move with the bindings.
    pub(super) fn apply_keys_change(&mut self) {
        let panel = self.panes[self.pane].shown_mut().panel_mut();
        let Some(view) = panel.as_any_mut().downcast_mut::<KeysView>() else {
            return;
        };
        if let Some(keymap) = view.take_changed() {
            self.keymap = keymap;
            self.mark_dirty();
        }
    }

    fn apply_setting(&mut self, key: &str, settings: &Settings, dir: Option<&std::path::Path>) {
        match key {
            // Both halves at once, from one load: the palette and the syntax
            // scopes never come from two different themes.
            "theme" => {
                let (colors, syntax, warning) =
                    crate::config::load_theme(dir, &settings.theme, self.depth);
                self.set_theme(colors);
                self.set_syntax_theme(syntax);
                if let Some(warning) = warning {
                    self.status = Some(warning);
                }
            }
            "indent_width" => {
                self.indent_width = settings.indent_width;
                if let Some(width) = settings.indent_width {
                    for tab in self.all_tabs_mut() {
                        if let Some(editor) = tab.editor_mut() {
                            editor.set_tab_width(width);
                        }
                    }
                }
            }
            "whitespace" => {
                self.whitespace = settings.whitespace;
                for tab in self.all_tabs_mut() {
                    if let Some(editor) = tab.editor_mut() {
                        editor.set_whitespace(settings.whitespace);
                    }
                }
            }
            "layout" => self.set_preset(settings.layout),
            "adapt" => self.set_adapt(settings.adapt),
            "session" => self.session_enabled = settings.session,
            // Restart settings, said so by the tab.
            _ => {}
        }
        self.mark_dirty();
    }
}
