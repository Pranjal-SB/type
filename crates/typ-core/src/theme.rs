//! Reading a theme out of a TOML file.
//!
//! ```toml
//! name = "TYPE Slate"
//! kind = "dark"
//!
//! [palette]            # names, so retuning a port is one edit not nineteen
//! base00 = "#10141b"
//! accent = "#4f8cc9"
//!
//! [ui]                 # the 25 ThemeColors fields, by their own names
//! fg = "base07"
//! bg = "#10141b"       # a literal is always allowed
//!
//! [syntax]             # tree-sitter capture names. Parsed now, read from M2.6.
//! keyword  = { fg = "mauve", modifiers = ["bold"] }
//! function = "blue"    # a bare string means fg
//! ```
//!
//! **`[ui]` is typed and `[syntax]` is open, because they are different kinds of
//! thing.** The UI slots are a closed record known at compile time; scopes are an
//! open set of strings a grammar TYPE has never seen can add to. Helix puts both
//! in one flat namespace, and the cost is that a typo in a `ui.` key is silently
//! ignored — the theme renders wrong and you go looking. Here an unknown `[ui]`
//! key is a load error naming the key it probably meant.
//!
//! **A key the file does not mention keeps the shipped value.** A theme is a set
//! of overrides, exactly as `keys.toml` is. Without that rule every theme in the
//! world breaks the first time a colour is added, and every file has to name all
//! twenty-four to say anything at all.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use ratatui::style::{Color, Modifier, Style};

use crate::ThemeColors;

/// Which ground a palette is drawn on.
///
/// Stated by the author rather than inferred from the background's luminance:
/// it decides one of the contrast floors, and a guess that disagrees with what
/// the author meant is a disagreement nobody can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dark,
    Light,
}

impl Kind {
    fn parse(value: &str) -> Result<Kind> {
        match value {
            "dark" => Ok(Kind::Dark),
            "light" => Ok(Kind::Light),
            other => bail!("kind must be \"dark\" or \"light\", not {other:?}"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Dark => "dark",
            Kind::Light => "light",
        }
    }
}

/// Styles for tree-sitter capture names.
///
/// **Nothing reads this until M2.6**, which owns it. It is parsed and validated
/// now because the alternative is a breaking change to every shipped theme and
/// every community theme the moment the highlighter lands — the same argument
/// that put the four `diagnostic_*` fields in `ThemeColors` before M3.
///
/// It is a separate type from [`ThemeColors`] rather than a field on it, because
/// `ThemeColors` is `Copy` and is held by value in the app and by reference in
/// four render paths. A map inside it would drop `Copy` and ripple through all
/// of them for the benefit of a consumer that does not exist yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyntaxTheme {
    scopes: BTreeMap<String, Style>,
}

impl SyntaxTheme {
    /// The style for a capture name, falling back to its longest defined
    /// prefix.
    ///
    /// `function.builtin.static` tries `function.builtin.static`, then
    /// `function.builtin`, then `function`. That is what lets a fourteen-line
    /// theme colour a grammar it has never heard of.
    ///
    /// Prefixes are cut at dots rather than at bytes, so `functional` is not a
    /// `function`.
    pub fn get(&self, scope: &str) -> Option<Style> {
        let mut candidate = scope;
        loop {
            if let Some(style) = self.scopes.get(candidate) {
                return Some(*style);
            }
            candidate = candidate.rsplit_once('.')?.0;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}

/// A loaded theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub kind: Kind,
    pub colors: ThemeColors,
    pub syntax: SyntaxTheme,
}

impl Theme {
    /// Parse a theme file.
    ///
    /// **All or nothing.** Everything is resolved before anything is assigned,
    /// so a file with one bad line leaves the caller with the palette it already
    /// had. `Keymap::merge_toml` holds the same rule for the same reason: a
    /// half-applied config is worse than a rejected one, because the user cannot
    /// tell which half took effect.
    pub fn from_toml(source: &str) -> Result<Theme> {
        let table: toml::Table = toml::from_str(source).context("parsing the theme")?;

        let name = table
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| anyhow!("a theme needs a `name`"))?
            .to_string();

        let kind = table
            .get("kind")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| anyhow!("a theme needs a `kind`, \"dark\" or \"light\""))
            .and_then(Kind::parse)?;

        let palette = parse_palette(&table)?;
        let colors = parse_ui(&table, &palette)?;
        let syntax = parse_syntax(&table, &palette)?;

        Ok(Theme {
            name,
            kind,
            colors,
            syntax,
        })
    }

    /// Render a palette back out as a theme file.
    ///
    /// This is how the shipped default becomes a file rather than a private
    /// path, and it is what keeps `every_ui_key_the_editor_has_can_be_set_from_a_file`
    /// honest: the emitter destructures `ThemeColors` exhaustively, so a new
    /// colour cannot be added without appearing here, and the round-trip then
    /// proves the parser accepts everything the emitter writes.
    pub fn write_toml(name: &str, kind: Kind, colors: &ThemeColors) -> String {
        let mut out = format!("name = {name:?}\nkind = {:?}\n\n[ui]\n", kind.label());
        for (key, colour) in ui_pairs(colors) {
            out.push_str(&format!("{key} = \"{}\"\n", hex_of(colour)));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------

fn hex_of(colour: Color) -> String {
    match colour {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        // Nothing in a shipped palette is anything else — the audit asserts it —
        // and a theme file has no syntax for a named colour.
        other => unreachable!("{other:?} cannot be written to a theme file"),
    }
}

fn parse_hex(key: &str, value: &str) -> Result<Color> {
    let digits = value
        .strip_prefix('#')
        .filter(|rest| rest.len() == 6 && rest.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| {
            anyhow!("{key}: {value:?} is not a colour — six hex digits after a #, like \"#4f8cc9\"")
        })?;
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16);
    Ok(Color::Rgb(
        channel(0).expect("six hex digits were checked"),
        channel(2).expect("six hex digits were checked"),
        channel(4).expect("six hex digits were checked"),
    ))
}

/// A literal, or a name from `[palette]`.
///
/// Palette entries are themselves always literals. Letting one name another
/// would invite a cycle for the sake of saving a line in a file nobody writes
/// twice.
fn resolve(key: &str, value: &str, palette: &BTreeMap<String, Color>) -> Result<Color> {
    if value.starts_with('#') {
        return parse_hex(key, value);
    }
    palette
        .get(value)
        .copied()
        .ok_or_else(|| anyhow!("{key}: {value:?} is not a colour and is not in [palette]"))
}

fn parse_palette(table: &toml::Table) -> Result<BTreeMap<String, Color>> {
    let Some(section) = table.get("palette") else {
        return Ok(BTreeMap::new());
    };
    let section = section
        .as_table()
        .ok_or_else(|| anyhow!("[palette] must be a table of name = \"#rrggbb\""))?;

    let mut palette = BTreeMap::new();
    for (name, value) in section {
        let text = value
            .as_str()
            .ok_or_else(|| anyhow!("palette.{name} must be a string"))?;
        palette.insert(name.clone(), parse_hex(&format!("palette.{name}"), text)?);
    }
    Ok(palette)
}

// ---------------------------------------------------------------------------
// [ui]
// ---------------------------------------------------------------------------

/// Every UI slot, paired with its value.
///
/// Destructured exhaustively and without `..` on purpose: a field added to
/// `ThemeColors` fails to compile here until it is given a name, which is what
/// stops a new colour from being unreachable from a theme file.
pub(crate) fn ui_pairs(colors: &ThemeColors) -> [(&'static str, Color); 25] {
    let ThemeColors {
        fg,
        bg,
        cursor_line_bg,
        gutter_fg,
        gutter_bg,
        line_number_fg,
        line_number_current_fg,
        selection_bg,
        selection_fg,
        selection_primary_bg,
        bracket_match_fg,
        bracket_match_bg,
        chrome_bg,
        border,
        border_focused,
        status_bar_bg,
        status_bar_fg,
        status_bar_inactive_fg,
        status_bar_accent,
        tree_directory_fg,
        tree_file_fg,
        diagnostic_error,
        diagnostic_warning,
        diagnostic_info,
        diagnostic_hint,
    } = *colors;

    [
        ("fg", fg),
        ("bg", bg),
        ("cursor_line_bg", cursor_line_bg),
        ("gutter_fg", gutter_fg),
        ("gutter_bg", gutter_bg),
        ("line_number_fg", line_number_fg),
        ("line_number_current_fg", line_number_current_fg),
        ("selection_bg", selection_bg),
        ("selection_fg", selection_fg),
        ("selection_primary_bg", selection_primary_bg),
        ("bracket_match_fg", bracket_match_fg),
        ("bracket_match_bg", bracket_match_bg),
        ("chrome_bg", chrome_bg),
        ("border", border),
        ("border_focused", border_focused),
        ("status_bar_bg", status_bar_bg),
        ("status_bar_fg", status_bar_fg),
        ("status_bar_inactive_fg", status_bar_inactive_fg),
        ("status_bar_accent", status_bar_accent),
        ("tree_directory_fg", tree_directory_fg),
        ("tree_file_fg", tree_file_fg),
        ("diagnostic_error", diagnostic_error),
        ("diagnostic_warning", diagnostic_warning),
        ("diagnostic_info", diagnostic_info),
        ("diagnostic_hint", diagnostic_hint),
    ]
}

fn assign(colors: &mut ThemeColors, key: &str, colour: Color) -> bool {
    match key {
        "fg" => colors.fg = colour,
        "bg" => colors.bg = colour,
        "cursor_line_bg" => colors.cursor_line_bg = colour,
        "gutter_fg" => colors.gutter_fg = colour,
        "gutter_bg" => colors.gutter_bg = colour,
        "chrome_bg" => colors.chrome_bg = colour,
        "line_number_fg" => colors.line_number_fg = colour,
        "line_number_current_fg" => colors.line_number_current_fg = colour,
        "selection_bg" => colors.selection_bg = colour,
        "selection_fg" => colors.selection_fg = colour,
        "selection_primary_bg" => colors.selection_primary_bg = colour,
        "bracket_match_fg" => colors.bracket_match_fg = colour,
        "bracket_match_bg" => colors.bracket_match_bg = colour,
        "border" => colors.border = colour,
        "border_focused" => colors.border_focused = colour,
        "status_bar_bg" => colors.status_bar_bg = colour,
        "status_bar_fg" => colors.status_bar_fg = colour,
        "status_bar_inactive_fg" => colors.status_bar_inactive_fg = colour,
        "status_bar_accent" => colors.status_bar_accent = colour,
        "tree_directory_fg" => colors.tree_directory_fg = colour,
        "tree_file_fg" => colors.tree_file_fg = colour,
        "diagnostic_error" => colors.diagnostic_error = colour,
        "diagnostic_warning" => colors.diagnostic_warning = colour,
        "diagnostic_info" => colors.diagnostic_info = colour,
        "diagnostic_hint" => colors.diagnostic_hint = colour,
        _ => return false,
    }
    true
}

/// Levenshtein distance, for suggesting the key an author meant.
///
/// Two rows rather than a full matrix: the strings are short and this runs once
/// per unknown key in a file somebody is in the middle of fixing.
fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b_chars.len()).collect();
    let mut current = vec![0usize; b_chars.len() + 1];

    for (i, a_char) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, b_char) in b_chars.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != *b_char);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b_chars.len()]
}

/// The nearest real key, if one is near enough to be worth naming.
///
/// A third of the key's length, so a long name tolerates a bigger slip than a
/// two-letter one. Beyond that a suggestion is noise: nothing sensible is close
/// to `wombat`, and offering `fg` would be worse than offering nothing.
fn nearest_key(unknown: &str) -> Option<&'static str> {
    let budget = (unknown.chars().count() / 3).max(1);
    ui_pairs(&ThemeColors::default())
        .into_iter()
        .map(|(key, _)| (key, edit_distance(unknown, key)))
        .filter(|(_, distance)| *distance <= budget)
        .min_by_key(|(_, distance)| *distance)
        .map(|(key, _)| key)
}

fn parse_ui(table: &toml::Table, palette: &BTreeMap<String, Color>) -> Result<ThemeColors> {
    let mut colors = ThemeColors::default();
    let Some(section) = table.get("ui") else {
        return Ok(colors);
    };
    let section = section
        .as_table()
        .ok_or_else(|| anyhow!("[ui] must be a table"))?;

    // Resolved into a staging list first, so one bad line changes nothing.
    let mut staged = Vec::with_capacity(section.len());
    for (key, value) in section {
        let text = value
            .as_str()
            .ok_or_else(|| anyhow!("ui.{key} must be a string"))?;
        if !is_ui_key(key) {
            return Err(match nearest_key(key) {
                Some(near) => {
                    anyhow!("ui.{key} is not a colour this editor has — did you mean {near}?")
                }
                None => anyhow!("ui.{key} is not a colour this editor has"),
            });
        }
        staged.push((key.clone(), resolve(&format!("ui.{key}"), text, palette)?));
    }

    for (key, colour) in staged {
        // `is_ui_key` reads `ui_pairs` and this reads a `match`, so the two
        // could in principle drift. They cannot drift silently: the round-trip
        // test writes every pair and parses it back, and this turns the
        // remaining gap into a loud failure rather than a colour that quietly
        // does not take.
        assert!(
            assign(&mut colors, &key, colour),
            "ui.{key} is a known key with no assignment arm — ui_pairs and assign have drifted"
        );
    }
    Ok(colors)
}

fn is_ui_key(key: &str) -> bool {
    ui_pairs(&ThemeColors::default())
        .iter()
        .any(|(name, _)| *name == key)
}

// ---------------------------------------------------------------------------
// [syntax]
// ---------------------------------------------------------------------------

fn parse_modifier(name: &str) -> Result<Modifier> {
    // Blink and hidden are deliberately absent. No editor uses them for syntax
    // and a theme that can make code invisible is a theme that will.
    match name {
        "bold" => Ok(Modifier::BOLD),
        "dim" => Ok(Modifier::DIM),
        "italic" => Ok(Modifier::ITALIC),
        "underlined" => Ok(Modifier::UNDERLINED),
        "reversed" => Ok(Modifier::REVERSED),
        "crossed_out" => Ok(Modifier::CROSSED_OUT),
        other => bail!(
            "{other:?} is not a modifier — bold, dim, italic, underlined, reversed or crossed_out"
        ),
    }
}

fn parse_syntax(table: &toml::Table, palette: &BTreeMap<String, Color>) -> Result<SyntaxTheme> {
    let Some(section) = table.get("syntax") else {
        return Ok(SyntaxTheme::default());
    };
    let section = section
        .as_table()
        .ok_or_else(|| anyhow!("[syntax] must be a table"))?;

    let mut scopes = BTreeMap::new();
    for (scope, value) in section {
        let style = match value {
            // The common case is a foreground and nothing else. Making it one
            // word is the difference between a theme file someone writes and
            // one they abandon.
            toml::Value::String(text) => {
                Style::default().fg(resolve(&format!("syntax.{scope}"), text, palette)?)
            }
            toml::Value::Table(fields) => {
                let mut style = Style::default();
                if let Some(fg) = fields.get("fg") {
                    let text = fg
                        .as_str()
                        .ok_or_else(|| anyhow!("syntax.{scope}.fg must be a string"))?;
                    style = style.fg(resolve(&format!("syntax.{scope}.fg"), text, palette)?);
                }
                if let Some(bg) = fields.get("bg") {
                    let text = bg
                        .as_str()
                        .ok_or_else(|| anyhow!("syntax.{scope}.bg must be a string"))?;
                    style = style.bg(resolve(&format!("syntax.{scope}.bg"), text, palette)?);
                }
                if let Some(modifiers) = fields.get("modifiers") {
                    let list = modifiers
                        .as_array()
                        .ok_or_else(|| anyhow!("syntax.{scope}.modifiers must be a list"))?;
                    for entry in list {
                        let name = entry.as_str().ok_or_else(|| {
                            anyhow!("syntax.{scope}.modifiers must be a list of strings")
                        })?;
                        style = style.add_modifier(
                            parse_modifier(name).with_context(|| format!("syntax.{scope}"))?,
                        );
                    }
                }
                style
            }
            _ => bail!("syntax.{scope} must be a colour or a table"),
        };
        scopes.insert(scope.clone(), style);
    }

    Ok(SyntaxTheme { scopes })
}
