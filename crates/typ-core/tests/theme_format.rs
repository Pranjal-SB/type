//! Reading a theme out of a TOML file.
//!
//! The rules a palette must satisfy live in `theme.rs`; this is about whether a
//! file can express one, and what happens when it cannot.

use ratatui::style::{Color, Modifier};
use typ_core::ThemeColors;
use typ_core::theme::{Kind, Theme};

#[test]
fn a_theme_names_itself_and_says_which_ground_it_is_for() {
    let theme = Theme::from_toml(
        r##"
        name = "Slate"
        kind = "dark"
        "##,
    )
    .unwrap();

    assert_eq!(theme.name, "Slate");
    assert_eq!(theme.kind, Kind::Dark);
}

#[test]
fn colours_are_literals_or_names_from_the_palette() {
    let theme = Theme::from_toml(
        r##"
        name = "Two ways"
        kind = "dark"

        [palette]
        ink = "#c8d0dc"

        [ui]
        fg = "ink"
        bg = "#10141b"
        "##,
    )
    .unwrap();

    assert_eq!(theme.colors.fg, Color::Rgb(0xc8, 0xd0, 0xdc));
    assert_eq!(theme.colors.bg, Color::Rgb(0x10, 0x14, 0x1b));
}

#[test]
fn a_key_the_file_does_not_mention_keeps_the_shipped_value() {
    // A theme is a set of overrides, exactly as keys.toml is. Without this, a
    // file would have to name all twenty-four to say anything at all, and every
    // new colour would break every theme in the world.
    let theme = Theme::from_toml(
        r##"
        name = "One change"
        kind = "dark"

        [ui]
        fg = "#ff0000"
        "##,
    )
    .unwrap();

    let shipped = ThemeColors::default();
    assert_eq!(theme.colors.fg, Color::Rgb(0xff, 0x00, 0x00));
    assert_eq!(theme.colors.bg, shipped.bg, "bg was never mentioned");
    assert_eq!(theme.colors.status_bar_accent, shipped.status_bar_accent);
}

#[test]
fn every_ui_key_the_editor_has_can_be_set_from_a_file() {
    // The format has to be able to express the palette the editor already
    // ships, or the shipped theme cannot become a file — which is the whole
    // point of the exercise. Round-tripped through the writer so this cannot
    // drift as fields are added.
    let source = Theme::write_toml("Round trip", Kind::Dark, &ThemeColors::default());
    let parsed = Theme::from_toml(&source).unwrap();

    assert_eq!(parsed.colors, ThemeColors::default());
}

// ---------------------------------------------------------------------------
// What a broken file does
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_ui_key_names_the_one_it_probably_meant() {
    // The realistic typo in a file with twenty-four long snake_case keys is a
    // near miss, not a wild guess. Helix's flat namespace silently ignores this
    // and the theme just renders wrong somewhere.
    let error = Theme::from_toml(
        r##"
        name = "Typo"
        kind = "dark"

        [ui]
        selection_primry_bg = "#ffffff"
        "##,
    )
    .unwrap_err();
    // Rendered the way the app will render it: `{:#}` walks the whole chain,
    // which is what `load_keymap` already does for a bad keys.toml. Plain
    // `to_string` shows only the outermost context and hides the actual
    // complaint.
    let error = format!("{error:#}");

    assert!(
        error.contains("selection_primry_bg"),
        "the error has to say which key: {error}"
    );
    assert!(
        error.contains("selection_primary_bg"),
        "and which one it probably meant: {error}"
    );
}

#[test]
fn an_unknown_ui_key_with_no_near_miss_still_fails_cleanly() {
    // Nothing sensible to suggest. It still has to be an error naming the key,
    // rather than a suggestion pulled from too far away.
    let error = Theme::from_toml(
        r##"
        name = "Nonsense"
        kind = "dark"

        [ui]
        wombat = "#ffffff"
        "##,
    )
    .unwrap_err();
    // Rendered the way the app will render it: `{:#}` walks the whole chain,
    // which is what `load_keymap` already does for a bad keys.toml. Plain
    // `to_string` shows only the outermost context and hides the actual
    // complaint.
    let error = format!("{error:#}");

    assert!(error.contains("wombat"), "{error}");
}

#[test]
fn a_malformed_colour_names_the_key_it_is_under() {
    let error = Theme::from_toml(
        r##"
        name = "Bad hex"
        kind = "dark"

        [ui]
        bg = "#12345"
        "##,
    )
    .unwrap_err();
    // Rendered the way the app will render it: `{:#}` walks the whole chain,
    // which is what `load_keymap` already does for a bad keys.toml. Plain
    // `to_string` shows only the outermost context and hides the actual
    // complaint.
    let error = format!("{error:#}");

    assert!(error.contains("bg"), "which key: {error}");
    assert!(
        error.contains("#12345"),
        "and what was wrong with it: {error}"
    );
}

#[test]
fn a_palette_name_that_does_not_exist_is_an_error() {
    let error = Theme::from_toml(
        r##"
        name = "Dangling"
        kind = "dark"

        [palette]
        ink = "#c8d0dc"

        [ui]
        fg = "inkk"
        "##,
    )
    .unwrap_err();
    // Rendered the way the app will render it: `{:#}` walks the whole chain,
    // which is what `load_keymap` already does for a bad keys.toml. Plain
    // `to_string` shows only the outermost context and hides the actual
    // complaint.
    let error = format!("{error:#}");

    assert!(error.contains("inkk"), "{error}");
}

#[test]
fn an_unknown_kind_is_an_error_rather_than_a_guess() {
    // Which ground a palette is for decides one of the audit's floors, so
    // guessing it from the background's luminance would be an invisible
    // disagreement with what the author wrote.
    assert!(Theme::from_toml("name = \"X\"\nkind = \"beige\"").is_err());
}

#[test]
fn one_bad_line_changes_nothing_at_all() {
    // The all-or-nothing rule keys.toml already holds. A half-applied theme is
    // worse than a rejected one, because the user cannot tell which half took.
    let source = r##"
        name = "Half bad"
        kind = "dark"

        [ui]
        fg = "#ffffff"
        bg = "not a colour"
    "##;

    assert!(Theme::from_toml(source).is_err());
}

// ---------------------------------------------------------------------------
// Syntax scopes — parsed now, read from M2.6
// ---------------------------------------------------------------------------

#[test]
fn a_bare_string_scope_means_a_foreground() {
    let theme = Theme::from_toml(
        r##"
        name = "Scopes"
        kind = "dark"

        [syntax]
        keyword = "#4f8cc9"
        "##,
    )
    .unwrap();

    let style = theme.syntax.get("keyword").expect("keyword was set");
    assert_eq!(style.fg, Some(Color::Rgb(0x4f, 0x8c, 0xc9)));
    assert_eq!(style.bg, None);
}

#[test]
fn a_scope_can_carry_a_background_and_modifiers() {
    let theme = Theme::from_toml(
        r##"
        name = "Scopes"
        kind = "dark"

        [palette]
        accent = "#4f8cc9"

        [syntax]
        keyword = { fg = "accent", bg = "#101010", modifiers = ["bold", "italic"] }
        "##,
    )
    .unwrap();

    let style = theme.syntax.get("keyword").unwrap();
    assert_eq!(style.fg, Some(Color::Rgb(0x4f, 0x8c, 0xc9)));
    assert_eq!(style.bg, Some(Color::Rgb(0x10, 0x10, 0x10)));
    assert!(style.add_modifier.contains(Modifier::BOLD));
    assert!(style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn a_scope_falls_back_to_its_longest_defined_prefix() {
    // A grammar TYPE has never seen emits scopes this theme never heard of.
    // Longest-prefix is what lets a fourteen-line theme still colour them.
    let theme = Theme::from_toml(
        r##"
        name = "Scopes"
        kind = "dark"

        [syntax]
        function = "#111111"
        "function.builtin" = "#222222"
        "##,
    )
    .unwrap();

    assert_eq!(
        theme.syntax.get("function.builtin.static").unwrap().fg,
        Some(Color::Rgb(0x22, 0x22, 0x22)),
        "function.builtin is a longer match than function"
    );
    assert_eq!(
        theme.syntax.get("function.method").unwrap().fg,
        Some(Color::Rgb(0x11, 0x11, 0x11)),
    );
    assert!(
        theme.syntax.get("comment").is_none(),
        "nothing matches, and inventing a colour would be worse than saying so"
    );
}

#[test]
fn a_prefix_match_stops_at_a_dot() {
    // "functional" is not a "function". Matching on raw string prefixes rather
    // than on scope segments would colour it as one.
    let theme = Theme::from_toml(
        r##"
        name = "Scopes"
        kind = "dark"

        [syntax]
        function = "#111111"
        "##,
    )
    .unwrap();

    assert!(theme.syntax.get("functional").is_none());
}

#[test]
fn an_unknown_modifier_is_an_error() {
    let error = Theme::from_toml(
        r##"
        name = "Scopes"
        kind = "dark"

        [syntax]
        keyword = { fg = "#ffffff", modifiers = ["sparkly"] }
        "##,
    )
    .unwrap_err();
    // Rendered the way the app will render it: `{:#}` walks the whole chain,
    // which is what `load_keymap` already does for a bad keys.toml. Plain
    // `to_string` shows only the outermost context and hides the actual
    // complaint.
    let error = format!("{error:#}");

    assert!(error.contains("sparkly"), "{error}");
}
