//! What a theme has to be true of, checked rather than asserted.
//!
//! A palette is the one part of an editor where "looks fine to me" is the usual
//! standard of proof, and it is the wrong one: legibility is a measurable
//! property and colour-blind users are not served by a designer's eye. These
//! tests compute WCAG contrast from the actual channel values, so a palette
//! change that makes text unreadable fails a build rather than shipping.
//!
//! The rubric lives in [`audit`] rather than in a dozen `#[test]` functions,
//! because two more callers arrive this milestone: the 256-colour degradation
//! has to be checked against the same rules, and so does every shipped theme
//! file. Three copies of a rubric is three rubrics as soon as one of them is
//! edited.

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

/// How far a colour sits from the ground it is drawn on.
///
/// This is what "stands out" and "recedes" actually mean, and it is the
/// substitution that lets one rule serve both kinds of palette.
/// `luminance(a) > luminance(b)` says the same thing only when the background
/// is dark: on a pale ground emphasis moves *down* in luminance and recession
/// moves up, so a rule written as a bare luminance comparison rejects a
/// perfectly correct light palette. `light_fixture` is that palette, and it is
/// here because four rules in this file used to do exactly that.
fn distance_from(colour: Color, ground: Color) -> f64 {
    (luminance(colour) - luminance(ground)).abs()
}

/// Whether the palette is drawn on a pale ground.
///
/// Only one rule needs to know — see the cursor-line floor in [`audit`].
fn is_light(theme: &ThemeColors) -> bool {
    luminance(theme.bg) > 0.5
}

// ---------------------------------------------------------------------------
// The rubric
// ---------------------------------------------------------------------------

/// Every rule a palette must satisfy, as a list of what it got wrong.
///
/// Empty means the palette is fine. Returning the whole list rather than
/// panicking on the first is deliberate: tuning a ported theme means seeing all
/// of its problems at once, not fixing one and rediscovering the next.
fn audit(theme: &ThemeColors) -> Vec<String> {
    let mut bad = Vec::new();

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
    let cursor_line_floor = if is_light(theme) { 6.5 } else { 7.0 };
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

#[track_caller]
fn assert_clean(name: &str, theme: &ThemeColors) {
    let bad = audit(theme);
    assert!(
        bad.is_empty(),
        "{name} fails {} of the palette rules:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A light palette that is *correct*, so the rules can be checked against one.
///
/// Four of the rules above used to compare luminance directly, which encodes
/// "the ground is dark" into statements that are really about standing out from
/// the ground. Every one of them rejected this palette while the colours were
/// doing exactly the right thing.
///
/// Two of these values took searching, and both are the same lesson in
/// miniature. `selection_primary_bg` has to stay 4.5 against the text while
/// still differing from `selection_bg` by 1.3, and on a pale ground those pull
/// against each other. The diagnostics are worse: an amber that clears 4.5 on
/// near-white and is still 1.8 in lightness from the error red lives in a
/// window about half a ratio point wide. That is why a ported light theme is an
/// adaptation rather than a copy.
fn light_fixture() -> ThemeColors {
    ThemeColors {
        fg: Color::Rgb(0x1a, 0x1c, 0x20),
        bg: Color::Rgb(0xfd, 0xfd, 0xfc),
        cursor_line_bg: Color::Rgb(0xf2, 0xf2, 0xf0),

        gutter_fg: Color::Rgb(0x8a, 0x8d, 0x93),
        gutter_bg: Color::Rgb(0xfd, 0xfd, 0xfc),
        // Lighter than the body text, not darker: on a pale ground, receding
        // means moving toward the background.
        line_number_fg: Color::Rgb(0x8a, 0x8d, 0x93),
        line_number_current_fg: Color::Rgb(0x1a, 0x1c, 0x20),

        selection_bg: Color::Rgb(0xd3, 0xdc, 0xea),
        selection_fg: Color::Rgb(0x1a, 0x1c, 0x20),
        selection_primary_bg: Color::Rgb(0xa6, 0xbf, 0xe2),

        bracket_match_fg: Color::Rgb(0x8a, 0x4b, 0x00),
        bracket_match_bg: Color::Rgb(0xfd, 0xf0, 0xd9),

        border: Color::Rgb(0xd8, 0xd8, 0xd4),
        // Darker than the unfocused border, for the same reason.
        border_focused: Color::Rgb(0x1f, 0x5f, 0xa8),

        status_bar_bg: Color::Rgb(0xf0, 0xf0, 0xed),
        status_bar_fg: Color::Rgb(0x1a, 0x1c, 0x20),
        status_bar_inactive_fg: Color::Rgb(0x5f, 0x62, 0x68),
        status_bar_accent: Color::Rgb(0x1f, 0x5f, 0xa8),

        tree_directory_fg: Color::Rgb(0x1f, 0x5f, 0xa8),
        tree_file_fg: Color::Rgb(0x3a, 0x3d, 0x43),

        diagnostic_error: Color::Rgb(0x8f, 0x14, 0x14),
        diagnostic_warning: Color::Rgb(0x99, 0x68, 0x00),
        diagnostic_info: Color::Rgb(0x1f, 0x5f, 0xa8),
        diagnostic_hint: Color::Rgb(0x0d, 0x6b, 0x6b),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
fn the_shipped_palette_satisfies_every_rule() {
    assert_clean("the default theme", &ThemeColors::default());
}

#[test]
fn a_correct_light_palette_satisfies_every_rule() {
    // The regression this file exists to prevent: four of the rules above were
    // written as luminance comparisons and rejected this palette outright.
    assert_clean("the light fixture", &light_fixture());
}

#[test]
fn the_rules_reject_a_palette_that_earns_it() {
    // The audit is only worth running if it can fail. Body text the same colour
    // as the page is the least arguable way to earn that.
    let mut theme = ThemeColors::default();
    theme.fg = theme.bg;

    let bad = audit(&theme);

    assert!(
        bad.iter().any(|f| f.starts_with("fg on bg")),
        "an invisible foreground has to be reported, got: {bad:?}"
    );
}

#[test]
fn the_cursor_line_floor_follows_the_ground() {
    // The one rule in `audit` that branches on the kind of palette, and the
    // branch is worth a test of its own: both probes below put `fg` at the same
    // ~6.8 against their own cursor line, and that single ratio is a pass on a
    // pale ground and a failure on a dark one.
    //
    // Only reachable when `fg on bg` is itself near 7. A palette with room to
    // spare cannot get here at all — the `cursor_line vs bg < 1.5` ceiling caps
    // how far the tint can travel before the text-legibility floor is in
    // danger. That is why the exemption is narrow enough to be worth making.
    let failed_on_cursor_line = |theme: &ThemeColors| {
        audit(theme)
            .iter()
            .any(|f| f.starts_with("fg on cursor_line_bg"))
    };

    // Catppuccin Latte's real values: body text at 7.06, so any tint at all
    // costs more than the dark floor allows.
    let mut pale = light_fixture();
    pale.fg = Color::Rgb(0x4c, 0x4f, 0x69);
    pale.bg = Color::Rgb(0xef, 0xf1, 0xf5);
    pale.cursor_line_bg = Color::Rgb(0xea, 0xec, 0xf1); // 6.76 against fg

    assert!(
        !failed_on_cursor_line(&pale),
        "6.76 clears the 6.5 floor a pale ground gets"
    );

    let dark = ThemeColors {
        fg: Color::Rgb(0x9e, 0xa4, 0xac),
        cursor_line_bg: Color::Rgb(0x17, 0x1c, 0x25), // 6.80 against fg
        ..ThemeColors::default()
    };

    assert!(
        failed_on_cursor_line(&dark),
        "6.80 misses the 7.0 floor a dark ground keeps — a dark palette has \
         somewhere else to go, so it does not get the exemption"
    );
}
