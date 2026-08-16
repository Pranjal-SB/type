//! What a theme has to be true of, checked rather than asserted.
//!
//! A palette is the one part of an editor where "looks fine to me" is the usual
//! standard of proof, and it is the wrong one: legibility is a measurable
//! property and colour-blind users are not served by a designer's eye. These
//! tests compute WCAG contrast from the actual channel values, so a palette
//! change that makes text unreadable fails a build rather than shipping.

use ratatui::style::Color;
use typ_core::ThemeColors;

/// The channel values of a truecolor colour.
///
/// Panics on anything else, which is the point: the 16-colour ANSI palette
/// means TYPE inherits whatever blue the user's terminal defines and cannot be
/// tuned at all.
fn rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        other => panic!("{other:?} is not truecolor"),
    }
}

/// WCAG 2.1 relative luminance.
fn luminance(color: Color) -> f64 {
    fn channel(value: u8) -> f64 {
        let s = value as f64 / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    let (r, g, b) = rgb(color);
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG 2.1 contrast ratio, 1.0 (identical) to 21.0 (black on white).
fn contrast(a: Color, b: Color) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

fn assert_contrast(name: &str, fg: Color, bg: Color, floor: f64) {
    let ratio = contrast(fg, bg);
    assert!(
        ratio >= floor,
        "{name}: contrast {ratio:.2} is below the {floor:.1} floor"
    );
}

#[test]
fn every_colour_in_the_theme_is_truecolor() {
    // Destructured exhaustively and without `..` on purpose: adding a field to
    // `ThemeColors` breaks this test at compile time, which is how a new colour
    // is forced to be a considered one rather than a `Color::Blue` that nobody
    // notices until it clashes on someone else's terminal.
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
    } = ThemeColors::default();

    for (name, colour) in [
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
    ] {
        // `rgb` panics on anything that is not Color::Rgb.
        let _ = rgb(colour);
        assert!(
            luminance(colour) >= 0.0,
            "{name} produced a nonsensical luminance"
        );
    }
}

#[test]
fn body_text_is_comfortably_readable() {
    let theme = ThemeColors::default();
    // AAA for body text. This is the colour pair a user stares at all day.
    assert_contrast("fg on bg", theme.fg, theme.bg, 7.0);
}

#[test]
fn text_stays_readable_on_both_kinds_of_selection() {
    let theme = ThemeColors::default();
    assert_contrast(
        "selection_fg on selection_bg",
        theme.selection_fg,
        theme.selection_bg,
        4.5,
    );
    assert_contrast(
        "selection_fg on selection_primary_bg",
        theme.selection_fg,
        theme.selection_primary_bg,
        4.5,
    );
}

#[test]
fn the_primary_selection_is_distinguishable_from_the_others() {
    let theme = ThemeColors::default();
    // Helix themes `ui.selection.primary` separately for exactly this reason:
    // every motion is relative to the primary, and with thirty cursors there
    // has to be something saying which one that is.
    assert_ne!(theme.selection_primary_bg, theme.selection_bg);
    let ratio = contrast(theme.selection_primary_bg, theme.selection_bg);
    assert!(
        ratio >= 1.3,
        "the two selection backgrounds differ by only {ratio:.2}, which reads as \
         one colour under any gamma the user's terminal picks"
    );
}

#[test]
fn the_current_lines_number_stands_out_from_the_rest() {
    let theme = ThemeColors::default();
    assert_ne!(theme.line_number_current_fg, theme.line_number_fg);
    assert!(
        luminance(theme.line_number_current_fg) > luminance(theme.line_number_fg),
        "the current line's number must be the brighter of the two — a dimmer \
         'here' reads as disabled"
    );
}

#[test]
fn line_numbers_recede_without_becoming_unreadable() {
    let theme = ThemeColors::default();
    // 3:1 is the WCAG floor for non-body text. Below it the gutter stops being
    // information and becomes texture.
    assert_contrast("line_number_fg on bg", theme.line_number_fg, theme.bg, 3.0);
    assert!(
        luminance(theme.line_number_fg) < luminance(theme.fg),
        "line numbers must be quieter than the code they label"
    );
}

#[test]
fn the_current_line_highlight_is_felt_rather_than_seen() {
    let theme = ThemeColors::default();
    assert_ne!(theme.cursor_line_bg, theme.bg, "it has to do something");
    let ratio = contrast(theme.cursor_line_bg, theme.bg);
    assert!(
        ratio < 1.5,
        "cursor_line_bg is {ratio:.2} against the background, which is a stripe \
         across the screen rather than a hint at where the cursor is"
    );
    // And it must not eat the text sitting on it.
    assert_contrast("fg on cursor_line_bg", theme.fg, theme.cursor_line_bg, 7.0);
}

#[test]
fn a_matched_bracket_is_visible_against_its_own_background() {
    let theme = ThemeColors::default();
    assert_contrast(
        "bracket_match_fg on bracket_match_bg",
        theme.bracket_match_fg,
        theme.bracket_match_bg,
        4.5,
    );
}

#[test]
fn the_focused_border_is_brighter_than_an_unfocused_one() {
    let theme = ThemeColors::default();
    assert!(
        luminance(theme.border_focused) > luminance(theme.border),
        "focus is indicated by gaining attention, not losing it"
    );
}

#[test]
fn the_status_bar_reads_against_its_own_background() {
    let theme = ThemeColors::default();
    assert_contrast(
        "status_bar_fg",
        theme.status_bar_fg,
        theme.status_bar_bg,
        4.5,
    );
    // Inactive is quieter but still legible: it carries real content — the
    // filetype, the line ending — not decoration.
    assert_contrast(
        "status_bar_inactive_fg",
        theme.status_bar_inactive_fg,
        theme.status_bar_bg,
        3.0,
    );
    assert!(
        luminance(theme.status_bar_inactive_fg) < luminance(theme.status_bar_fg),
        "the inactive colour must be the quieter one"
    );
}

#[test]
fn the_tree_distinguishes_directories_from_files() {
    let theme = ThemeColors::default();
    assert_ne!(theme.tree_directory_fg, theme.tree_file_fg);
    assert_contrast("tree_directory_fg", theme.tree_directory_fg, theme.bg, 4.5);
    assert_contrast("tree_file_fg", theme.tree_file_fg, theme.bg, 4.5);
}

#[test]
fn every_diagnostic_severity_is_readable() {
    let theme = ThemeColors::default();
    for (name, colour) in [
        ("error", theme.diagnostic_error),
        ("warning", theme.diagnostic_warning),
        ("info", theme.diagnostic_info),
        ("hint", theme.diagnostic_hint),
    ] {
        assert_contrast(name, colour, theme.bg, 4.5);
    }
}

#[test]
fn error_and_warning_differ_by_more_than_hue() {
    let theme = ThemeColors::default();
    // Red and amber are the classic deuteranopia collision, and error-versus-
    // warning is the one diagnostic distinction that changes what a user does.
    // Separating them by lightness as well as hue is what keeps that decision
    // available to a red-green colour-blind reader.
    let ratio = contrast(theme.diagnostic_error, theme.diagnostic_warning);
    assert!(
        ratio >= 1.8,
        "error and warning differ by only {ratio:.2} in lightness, so they are \
         one colour to a red-green colour-blind reader"
    );
}
