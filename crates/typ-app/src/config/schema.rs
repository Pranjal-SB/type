//! Every setting in `config.toml`, one row each.
//!
//! The one description of a setting: parsing reads it, the defaults are
//! checked against it, and the settings tab draws it. A setting cannot exist in
//! one of those and not the others, which is what the keymap's action table
//! already does for the menus.

use typ_core::Depth;
use typ_panel_editor::render::Whitespace;

use super::Settings;
use crate::dock::Preset;

/// What kind of value a setting takes, and so which control edits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One of these names.
    Choice(&'static [&'static str]),
    /// A theme: the shipped names and whatever is in the themes directory.
    Theme,
    /// `true` or `false`.
    Toggle,
    /// A whole number in a range, or `auto` for unset.
    Number { min: i64, max: i64 },
    /// Edited in the file, not in the tab: a table with more than one value.
    File,
}

/// One setting.
pub struct Row {
    /// The key in `config.toml`.
    pub key: &'static str,
    /// Where it is grouped, as the settings tab's rules name it.
    pub section: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    /// The default as [`Row::show`] spells it.
    pub default: &'static str,
    /// It takes effect on the next start, not live.
    pub restart: bool,
    /// Read a value from `config.toml` into `settings`, or say what is wrong
    /// with it.
    pub parse: fn(&toml::Value, &mut Settings) -> Result<(), String>,
    /// The value in `settings`, as the tab shows it and `config.toml` spells it.
    pub show: fn(&Settings) -> String,
}

/// Every setting, in the order the tab lists them.
pub const ROWS: &[Row] = &[
    Row {
        key: "theme",
        section: "appearance",
        description: "the colour theme, shipped or from the themes directory",
        kind: Kind::Theme,
        default: "slate",
        restart: false,
        parse: |value, settings| match value.as_str() {
            Some(name) => {
                settings.theme = name.to_string();
                Ok(())
            }
            None => Err("theme must be a name in quotes".to_string()),
        },
        show: |settings| settings.theme.clone(),
    },
    Row {
        key: "color_depth",
        section: "appearance",
        description: "colours the terminal is sent; auto asks the terminal",
        kind: Kind::Choice(&["auto", "truecolor", "256"]),
        default: "auto",
        restart: true,
        parse: |value, settings| match value.as_str().and_then(parse_depth) {
            Some(depth) => {
                settings.color_depth = Some(depth);
                Ok(())
            }
            None => Err(format!(
                "color_depth {value} is not \"truecolor\" or \"256\""
            )),
        },
        show: |settings| {
            match settings.color_depth {
                None => "auto",
                Some(Depth::TrueColor) => "truecolor",
                Some(Depth::Ansi256) => "256",
            }
            .to_string()
        },
    },
    Row {
        key: "indent_width",
        section: "editor \u{203a} indentation",
        description: "columns per indent; auto measures each file",
        kind: Kind::Number { min: 1, max: 16 },
        default: "auto",
        restart: false,
        // Zero is a width nothing can insert, so it is rejected rather than
        // clamped: a setting that appears to take effect and does not is
        // worse than one that says it was ignored.
        parse: |value, settings| match value.as_integer().filter(|n| (1..=16).contains(n)) {
            Some(width) => {
                settings.indent_width = Some(width as usize);
                Ok(())
            }
            None => Err(format!(
                "indent_width {value} is not a whole number of columns from 1 to 16"
            )),
        },
        show: |settings| {
            settings
                .indent_width
                .map_or_else(|| "auto".to_string(), |width| width.to_string())
        },
    },
    Row {
        key: "whitespace",
        section: "editor \u{203a} whitespace",
        description: "which spaces and tabs get a visible mark",
        kind: Kind::Choice(WHITESPACE),
        default: "selection",
        restart: false,
        parse: |value, settings| match value.as_str().and_then(parse_whitespace) {
            Some(which) => {
                settings.whitespace = which;
                Ok(())
            }
            None => Err(format!(
                "whitespace {value} is not \"none\", \"trailing\", \"selection\" or \"all\""
            )),
        },
        show: |settings| whitespace_name(settings.whitespace).to_string(),
    },
    Row {
        key: "layout",
        section: "layout",
        description: "the layout preset to start in",
        kind: Kind::Choice(&["focus", "standard"]),
        default: "focus",
        restart: false,
        parse: |value, settings| match value.as_str().and_then(Preset::from_name) {
            Some(preset) => {
                settings.layout = preset;
                Ok(())
            }
            None => Err(format!("layout {value} is not \"focus\" or \"standard\"")),
        },
        show: |settings| settings.layout.name().to_string(),
    },
    Row {
        key: "adapt",
        section: "layout",
        description: "under 100 columns a standard layout folds its dock",
        kind: Kind::Toggle,
        default: "true",
        restart: false,
        parse: |value, settings| match value.as_bool() {
            Some(adapt) => {
                settings.adapt = adapt;
                Ok(())
            }
            None => Err(format!("adapt {value} is not true or false")),
        },
        show: |settings| settings.adapt.to_string(),
    },
    Row {
        key: "session",
        section: "layout",
        description: "quitting on a project remembers its panes and tabs",
        kind: Kind::Toggle,
        default: "true",
        restart: false,
        parse: |value, settings| match value.as_bool() {
            Some(session) => {
                settings.session = session;
                Ok(())
            }
            None => Err(format!("session {value} is not true or false")),
        },
        show: |settings| settings.session.to_string(),
    },
    Row {
        key: "language",
        section: "language servers",
        description: "servers by language, as [[language]] tables in the file",
        kind: Kind::File,
        default: "rust, toml",
        restart: true,
        // Read as a whole by `lsp::config::load`: an array of tables is not
        // one value with one answer, and each entry stands or falls alone.
        parse: |_, _| Ok(()),
        show: |settings| {
            settings
                .language_servers
                .iter()
                .map(|server| server.language_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        },
    },
];

const WHITESPACE: &[&str] = &["none", "trailing", "selection", "all"];

fn whitespace_name(which: Whitespace) -> &'static str {
    match which {
        Whitespace::None => "none",
        Whitespace::Trailing => "trailing",
        Whitespace::Selection => "selection",
        Whitespace::All => "all",
    }
}

fn parse_whitespace(value: &str) -> Option<Whitespace> {
    match value {
        "none" => Some(Whitespace::None),
        "trailing" => Some(Whitespace::Trailing),
        "selection" => Some(Whitespace::Selection),
        "all" => Some(Whitespace::All),
        _ => None,
    }
}

fn parse_depth(value: &str) -> Option<Depth> {
    match value {
        "truecolor" | "24bit" => Some(Depth::TrueColor),
        "256" | "ansi256" => Some(Depth::Ansi256),
        _ => None,
    }
}

/// The row for `key`.
pub fn row(key: &str) -> Option<&'static Row> {
    ROWS.iter().find(|row| row.key == key)
}
