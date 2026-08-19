//! What a palette has to be true of, checked rather than asserted.
//!
//! A palette is the one part of an editor where "looks fine to me" is the usual
//! standard of proof, and it is the wrong one: legibility is a measurable
//! property and colour-blind readers are not served by a designer's eye. Every
//! rule here computes WCAG 2.1 contrast from the actual channel values, so a
//! palette that cannot be read fails a build rather than shipping.
//!
//! Public because three callers need it and one of them is not a test: every
//! shipped theme is checked at both colour depths, and a theme author writing
//! their own file needs the same answer the project holds itself to. A rubric
//! that only the project can run is a rubric community themes ignore.

use ratatui::style::Color;

use crate::ThemeColors;
use crate::theme::Kind;

/// The channel values of a truecolor colour.
fn channels(colour: Color) -> Option<(u8, u8, u8)> {
    match colour {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => None,
    }
}

/// WCAG 2.1 relative luminance.
fn luminance(colour: Color) -> f64 {
    fn channel(value: u8) -> f64 {
        let c = value as f64 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    // A non-truecolor value is reported by its own rule; treating it as black
    // here keeps one bad colour from burying every other complaint in noise.
    let (r, g, b) = channels(colour).unwrap_or((0, 0, 0));
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG 2.1 contrast ratio, 1.0 (identical) to 21.0 (black on white).
fn contrast(a: Color, b: Color) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// How far a colour sits from the ground it is drawn on.
///
/// This is what "stands out" and "recedes" actually mean, and it is what lets
/// one rule serve both kinds of palette. `luminance(a) > luminance(b)` says the
/// same thing only when the ground is dark: on a pale ground emphasis moves
/// *down* in luminance and recession moves up, so a rule written as a bare
/// luminance comparison rejects a perfectly correct light palette.
fn distance_from(colour: Color, ground: Color) -> f64 {
    (luminance(colour) - luminance(ground)).abs()
}

/// Every rule a palette must satisfy, as a list of what it got wrong.
///
/// Empty means the palette is fine. Returning the whole list rather than
/// stopping at the first is deliberate: tuning a ported theme means seeing all
/// of its problems at once, not fixing one and rediscovering the next.
///
/// `kind` is what the theme file *declares*, not what its background happens to
/// look like — and one of the rules is that the two agree, so a `kind = "dark"`
/// theme with a pale page is caught rather than quietly audited against the
/// wrong floor.
pub fn audit(theme: &ThemeColors, kind: Kind) -> Vec<String> {
    let mut bad = Vec::new();

    // Every colour has to be a value TYPE chose. The 16-colour ANSI names mean
    // inheriting whatever the user's terminal defines, which cannot be measured
    // from here and cannot be tuned at all.
    for (name, colour) in super::theme::ui_pairs(theme) {
        if channels(colour).is_none() {
            bad.push(format!("{name}: {colour:?} is not a truecolor value"));
        }
    }

    // A palette that says it is one thing and looks like another is audited
    // against the wrong floor, silently.
    let looks_light = luminance(theme.bg) > 0.5;
    if looks_light != matches!(kind, Kind::Light) {
        bad.push(format!(
            "kind is {:?} but the background {:?} is {}",
            kind.label(),
            theme.bg,
            if looks_light { "pale" } else { "dark" }
        ));
    }

    // AAA for body text. This is the colour pair a user stares at all day.
    at_least(&mut bad, "fg on bg", theme.fg, theme.bg, 7.0);

    at_least(
        &mut bad,
        "selection_fg on selection_bg",
        theme.selection_fg,
        theme.selection_bg,
        4.5,
    );
    at_least(
        &mut bad,
        "selection_fg on selection_primary_bg",
        theme.selection_fg,
        theme.selection_primary_bg,
        4.5,
    );

    // Helix themes `ui.selection.primary` separately for exactly this reason:
    // every motion is relative to the primary, and with thirty cursors there
    // has to be something saying which one that is.
    differ(
        &mut bad,
        "selection_primary_bg vs selection_bg",
        theme.selection_primary_bg,
        theme.selection_bg,
        "the primary selection has to be tellable from the others",
    );
    at_least(
        &mut bad,
        "selection_primary_bg vs selection_bg",
        theme.selection_primary_bg,
        theme.selection_bg,
        1.3,
    );

    differ(
        &mut bad,
        "line_number_current_fg vs line_number_fg",
        theme.line_number_current_fg,
        theme.line_number_fg,
        "the current line's number has to be marked somehow",
    );
    emphasised(
        &mut bad,
        "line_number_current_fg",
        theme.line_number_current_fg,
        theme.line_number_fg,
        theme.bg,
        "the current line's number must be the one that stands out — a number \
         closer to the page than its neighbours reads as disabled",
    );

    // 3:1 is the WCAG floor for non-body text. Below it the gutter stops being
    // information and becomes texture.
    at_least(
        &mut bad,
        "line_number_fg on bg",
        theme.line_number_fg,
        theme.bg,
        3.0,
    );
    emphasised(
        &mut bad,
        "fg over line_number_fg",
        theme.fg,
        theme.line_number_fg,
        theme.bg,
        "line numbers must be quieter than the code they label",
    );

    differ(
        &mut bad,
        "cursor_line_bg vs bg",
        theme.cursor_line_bg,
        theme.bg,
        "the current-line highlight has to do something",
    );
    below(
        &mut bad,
        "cursor_line_bg vs bg",
        theme.cursor_line_bg,
        theme.bg,
        1.5,
        "which is a stripe across the screen rather than a hint at where the \
         cursor is",
    );
    // And it must not eat the text sitting on it. The floor drops half a point
    // on a pale ground, and the reason is arithmetic rather than taste: a light
    // palette whose body text clears AAA by a whisker — Catppuccin Latte sits at
    // 7.06 — has no room left for a tint of any strength. The alternative is a
    // light theme with no current-line highlight at all.
    let cursor_line_floor = if looks_light { 6.5 } else { 7.0 };
    at_least(
        &mut bad,
        "fg on cursor_line_bg",
        theme.fg,
        theme.cursor_line_bg,
        cursor_line_floor,
    );

    at_least(
        &mut bad,
        "bracket_match_fg on bracket_match_bg",
        theme.bracket_match_fg,
        theme.bracket_match_bg,
        4.5,
    );

    emphasised(
        &mut bad,
        "border_focused",
        theme.border_focused,
        theme.border,
        theme.bg,
        "focus is indicated by gaining attention, not losing it",
    );

    at_least(
        &mut bad,
        "status_bar_fg",
        theme.status_bar_fg,
        theme.status_bar_bg,
        4.5,
    );
    // Inactive is quieter but still legible: it carries real content — the
    // filetype, the line ending — not decoration.
    at_least(
        &mut bad,
        "status_bar_inactive_fg",
        theme.status_bar_inactive_fg,
        theme.status_bar_bg,
        3.0,
    );
    emphasised(
        &mut bad,
        "status_bar_fg over status_bar_inactive_fg",
        theme.status_bar_fg,
        theme.status_bar_inactive_fg,
        theme.status_bar_bg,
        "the inactive colour must be the quieter one",
    );

    differ(
        &mut bad,
        "tree_directory_fg vs tree_file_fg",
        theme.tree_directory_fg,
        theme.tree_file_fg,
        "the tree has to distinguish directories from files",
    );
    at_least(
        &mut bad,
        "tree_directory_fg on bg",
        theme.tree_directory_fg,
        theme.bg,
        4.5,
    );
    at_least(
        &mut bad,
        "tree_file_fg on bg",
        theme.tree_file_fg,
        theme.bg,
        4.5,
    );

    for (name, colour) in [
        ("diagnostic_error", theme.diagnostic_error),
        ("diagnostic_warning", theme.diagnostic_warning),
        ("diagnostic_info", theme.diagnostic_info),
        ("diagnostic_hint", theme.diagnostic_hint),
    ] {
        at_least(&mut bad, name, colour, theme.bg, 4.5);
    }

    // Red and amber are the classic deuteranopia collision, and error-versus-
    // warning is the one diagnostic distinction that changes what a user does.
    // Separating them by lightness as well as hue is what keeps that decision
    // available to a red-green colour-blind reader. Palettes designed for
    // harmony fail this and have to be adapted — Rosé Pine misses it by 0.03.
    at_least(
        &mut bad,
        "diagnostic_error vs diagnostic_warning",
        theme.diagnostic_error,
        theme.diagnostic_warning,
        1.8,
    );

    bad
}

fn at_least(bad: &mut Vec<String>, name: &str, a: Color, b: Color, floor: f64) {
    let ratio = contrast(a, b);
    if ratio < floor {
        bad.push(format!(
            "{name}: contrast {ratio:.2} is below the {floor:.1} floor"
        ));
    }
}

fn below(bad: &mut Vec<String>, name: &str, a: Color, b: Color, ceiling: f64, why: &str) {
    let ratio = contrast(a, b);
    if ratio >= ceiling {
        bad.push(format!(
            "{name}: contrast {ratio:.2} is at or above {ceiling:.1}, {why}"
        ));
    }
}

fn differ(bad: &mut Vec<String>, name: &str, a: Color, b: Color, why: &str) {
    if a == b {
        bad.push(format!("{name}: both are {a:?}, and {why}"));
    }
}

/// `stands_out` must be the one further from the ground the pair sits on.
fn emphasised(
    bad: &mut Vec<String>,
    name: &str,
    stands_out: Color,
    quiet: Color,
    ground: Color,
    why: &str,
) {
    let (far, near) = (
        distance_from(stands_out, ground),
        distance_from(quiet, ground),
    );
    if far <= near {
        bad.push(format!(
            "{name}: {why} (it sits {far:.3} from the ground, the quiet one {near:.3})"
        ));
    }
}
