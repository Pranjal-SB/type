//! `config.toml` — everything that is not a keybinding.

use std::path::Path;

use typ_core::Depth;

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "slate".to_string(),
            color_depth: None,
        }
    }
}

fn parse_depth(value: &str) -> Option<Depth> {
    match value {
        "truecolor" | "24bit" => Some(Depth::TrueColor),
        "256" | "ansi256" => Some(Depth::Ansi256),
        _ => None,
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
    let Ok(source) = std::fs::read_to_string(path) else {
        // No config is the normal case, not a problem worth a message.
        return (settings, None);
    };

    let table: toml::Table = match toml::from_str(&source) {
        Ok(table) => table,
        Err(e) => return (settings, Some(format!("{}: {e}", path.display()))),
    };

    let mut complaints: Vec<String> = Vec::new();

    for (key, value) in &table {
        match key.as_str() {
            "theme" => match value.as_str() {
                Some(name) => settings.theme = name.to_string(),
                None => complaints.push("theme must be a name in quotes".to_string()),
            },
            "color_depth" => match value.as_str().and_then(parse_depth) {
                Some(depth) => settings.color_depth = Some(depth),
                None => complaints.push(format!(
                    "color_depth {value} is not \"truecolor\" or \"256\""
                )),
            },
            other => complaints.push(format!("{other} is not a setting this editor has")),
        }
    }

    let warning =
        (!complaints.is_empty()).then(|| format!("{}: {}", path.display(), complaints.join("; ")));
    (settings, warning)
}
