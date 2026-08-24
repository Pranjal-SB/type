//! Every theme that ships, against the rules the project holds itself to.
//!
//! Six palettes can each rot independently, and three of them are adaptations
//! of somebody else's work rather than transcriptions. This is what makes that
//! safe: a port either clears the floors or it does not ship.
//!
//! Enumerated rather than listed. A test naming six themes goes on passing
//! forever after somebody adds a seventh.

use typ_app::config::theme::embedded;
use typ_core::{Depth, Theme, audit, colour::downgrade_theme};

#[test]
fn every_shipped_theme_is_legible_as_written() {
    let mut broken: Vec<String> = Vec::new();

    for (name, source) in embedded() {
        let theme = match Theme::from_toml(source) {
            Ok(theme) => theme,
            Err(e) => {
                broken.push(format!("{name} does not parse: {e:#}"));
                continue;
            }
        };
        for failure in audit(&theme.colors, theme.kind) {
            broken.push(format!("{name}: {failure}"));
        }
    }

    assert!(broken.is_empty(), "\n  {}", broken.join("\n  "));
}

#[test]
fn every_shipped_theme_is_still_legible_at_256_colours() {
    // Nobody in the field checks this, so nobody knows whether their theme
    // survives a terminal that cannot do truecolor. Quantising moves every
    // colour, and it moves them by different amounts.
    let mut broken: Vec<String> = Vec::new();

    for (name, source) in embedded() {
        let Ok(theme) = Theme::from_toml(source) else {
            continue; // the test above owns parse failures
        };
        let degraded = downgrade_theme(&theme.colors, Depth::Ansi256);
        for failure in audit(&degraded, theme.kind) {
            broken.push(format!("{name} at 256: {failure}"));
        }
    }

    assert!(broken.is_empty(), "\n  {}", broken.join("\n  "));
}

#[test]
fn the_shipped_set_covers_both_grounds() {
    // A light theme is the one thing the audit's own floors were not originally
    // written for, so shipping without one would leave that half untested by
    // anything real.
    let kinds: Vec<_> = embedded()
        .filter_map(|(_, source)| Theme::from_toml(source).ok())
        .map(|theme| theme.kind)
        .collect();

    assert!(
        kinds.contains(&typ_core::Kind::Dark) && kinds.contains(&typ_core::Kind::Light),
        "got {kinds:?}"
    );
}

/// The capture names every shipped theme has to answer for.
///
/// A floor, not a ceiling: `SyntaxTheme::get` falls back through dot-separated
/// prefixes, so a grammar emitting `keyword.control` or `string.escape`
/// resolves through `keyword` or `string` without the theme naming either.
///
/// `tag` and `attribute` are here because neither shares a prefix with
/// anything else on the list — markdown's fenced HTML emits tags and Rust's
/// `#[derive(..)]` emits attributes, and without a row of their own both
/// resolve to nothing at all. Helix and Zed both carry them as top-level
/// scopes for the same reason.
const REQUIRED_SCOPES: &[&str] = &[
    "keyword",
    "function",
    "type",
    "string",
    "comment",
    "number",
    "constant",
    "variable",
    "property",
    "operator",
    "punctuation",
    "tag",
    "attribute",
];

#[test]
fn every_shipped_theme_defines_the_scope_floor() {
    // Without this the milestone misses its own goal: the highlighter lands,
    // every lookup returns `None`, and every file renders exactly as it did
    // before. A parser fed nothing is the "theme field nothing reads" trap
    // running backwards — the reader exists and the data is missing.
    let mut broken: Vec<String> = Vec::new();

    for (name, source) in embedded() {
        let Ok(theme) = Theme::from_toml(source) else {
            continue; // the parse test owns parse failures
        };
        for scope in REQUIRED_SCOPES {
            if theme.syntax.get(scope).is_none() {
                broken.push(format!("{name} has no [syntax] entry reaching {scope}"));
            }
        }
    }

    assert!(broken.is_empty(), "\n  {}", broken.join("\n  "));
}

#[test]
fn a_shipped_theme_paints_a_comment_differently_from_a_keyword() {
    // The weakest thing that still proves a theme said something rather than
    // pointing every scope at one colour to satisfy the test above.
    let mut broken: Vec<String> = Vec::new();

    for (name, source) in embedded() {
        let Ok(theme) = Theme::from_toml(source) else {
            continue;
        };
        let comment = theme.syntax.get("comment").and_then(|s| s.fg);
        let keyword = theme.syntax.get("keyword").and_then(|s| s.fg);
        if comment == keyword {
            broken.push(format!("{name}: comment and keyword are both {comment:?}"));
        }
    }

    assert!(broken.is_empty(), "\n  {}", broken.join("\n  "));
}
