//! `config.toml` — everything that is not a keybinding.

use std::path::Path;

use typ_core::Depth;
use typ_panel_editor::render::Whitespace;

/// The settings a running editor reads.
///
/// Kept in its own file rather than folded into `keys.toml`, and the reason is
/// not that Zed and VS Code split theirs. **A keymap is a document you replace
/// wholesale** — somebody ships a vim-layer keymap and you drop the file in —
/// **while settings are lines you edit.** One file means replacing a keymap
/// silently clobbers every setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Which theme to load, by name.
    pub theme: String,
    /// Forced colour depth. `None` means ask the terminal.
    ///
    /// The escape hatch detection cannot provide: nothing in the environment
    /// separates a tmux configured to forward truecolor from one that mangles
    /// it, so the answer is a setting rather than a cleverer guess.
    pub color_depth: Option<Depth>,
    /// Indent width, overriding what the open file measures as.
    ///
    /// `None` means measure it. Detection is a heuristic and a file that mixes
    /// units can defeat it, so there has to be somewhere to state the answer
    /// that is not "edit the file until the heuristic agrees".
    pub indent_width: Option<usize>,
    /// Which whitespace gets a visible mark.
    ///
    /// Unlike the two above there is no `None` case: every value is a real
    /// answer, and "unset" already has a name — [`Whitespace::None`].
    pub whitespace: Whitespace,
    /// Language servers, the compiled-in defaults with `[[language]]` applied.
    ///
    /// Never empty: TYPE knows about rust-analyzer and taplo without being
    /// told, and a config file that mentions neither leaves both.
    pub language_servers: Vec<crate::lsp::ServerConfig>,
    /// The layout preset to start in.
    pub layout: crate::dock::Preset,
    /// Whether a Standard layout renders as Focus under
    /// [`crate::dock::ADAPT_WIDTH`] columns.
    pub adapt: bool,
    /// Whether quitting on a project remembers its layout for next time.
    pub session: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "slate".to_string(),
            color_depth: None,
            indent_width: None,
            whitespace: Whitespace::default(),
            language_servers: crate::lsp::config::defaults(),
            layout: crate::dock::Preset::default(),
            adapt: true,
            session: true,
        }
    }
}

/// Read `config.toml`, keeping whatever parsed and reporting whatever did not.
///
/// **Not all-or-nothing, unlike a keymap or a theme.** Those two are rejected
/// whole because a half-applied one leaves the user unable to tell which half
/// took effect. Settings are independent of each other: a bad `color_depth`
/// says nothing about whether `theme` was meant, and discarding both over one
/// typo helps nobody.
pub fn load_settings(path: Option<&Path>) -> (Settings, Option<String>) {
    let mut settings = Settings::default();
    let Some(path) = path else {
        return (settings, None);
    };
    let source = match super::read_config(path) {
        Ok(Some(source)) => source,
        // No config is the normal case, not a problem worth a message.
        Ok(None) => return (settings, None),
        Err(warning) => return (settings, Some(warning)),
    };

    let table: toml::Table = match toml::from_str(&source) {
        Ok(table) => table,
        Err(e) => return (settings, Some(format!("{}: {e}", path.display()))),
    };

    let mut complaints: Vec<String> = Vec::new();

    // Every key through its row in the table, so a setting cannot be parsed
    // here and be missing from the settings tab, or the other way round.
    for (key, value) in &table {
        match super::schema::row(key) {
            Some(row) => {
                if let Err(complaint) = (row.parse)(value, &mut settings) {
                    complaints.push(complaint);
                }
            }
            None => complaints.push(format!("{key} is not a setting this editor has")),
        }
    }

    let (servers, server_complaints) = crate::lsp::config::load(&table);
    settings.language_servers = servers;
    complaints.extend(server_complaints);

    let warning =
        (!complaints.is_empty()).then(|| format!("{}: {}", path.display(), complaints.join("; ")));
    (settings, warning)
}
