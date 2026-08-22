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

/// What a role has to clear, given the ground it is drawn on.
///
/// **One number cannot serve both grounds, and this is the correction for it.**
/// WCAG 2.1's ratio is not perceptually uniform across polarity: it overrates
/// light text on a dark ground and underrates dark text on a pale one. Measured
/// over 1,066 colour pairs from 97 published terminal palettes, a dark ground
/// returns about **2.5x** the ratio of a light ground at the same perceived
/// contrast, and the factor is stable from roughly Lc 30 to Lc 60 — which is
/// where a gutter and a diagnostic actually live. It compresses toward 1.0 at
/// the top, where both metrics saturate, so body text gets its own pair rather
/// than a multiplier.
///
/// **`content` is capped by the colour-blindness rule, not by the calibration.**
/// The table would put it near 8.7 on a dark ground, and that is unsatisfiable:
/// `diagnostic_error` and `diagnostic_warning` must also sit 1.8 apart from each
/// other, and two colours that are both that bright against a dark page cannot
/// be. At 8.7 the warning has to reach 15.66 against the page, which is a
/// near-white yellow that has stopped being amber. 7.0 is the highest value that
/// leaves the two diagnostics room to differ — and at 256 colours it is tighter
/// still. The cube quantises `#f47e86` and `#fcd690` onto `#ff8787` and
/// `#ffd787`, whose separation is 1.69 whatever the truecolor values were, so
/// the error colour has to be dark enough to land on a *different* cube cell.
/// 6.5 is the highest floor that leaves room for that, and it is Lc 46 — within
/// a point of the Lc 45 Zed ships as its own default minimum.
///
/// The consequence was not academic. Under a flat 3.0, Slate's gutter passed at
/// 3.35 and Catppuccin Latte's failed at 2.83 — and Latte's is the more legible
/// of the two by a factor of two. The rubric was rejecting the better colour.
///
/// **Why these numbers and not APCA's.** APCA is the algorithm that exposed
/// this, and it cannot ship here: it is licensed to the W3 for web content
/// only, falls back to AGPL v3 for anything else, and carries a pending patent.
/// So the bias it revealed is corrected with WCAG arithmetic instead, calibrated
/// against it once, offline. The measurement is in `docs/plans/m2.5-colour.md`.
struct Floors {
    /// Text the user reads for minutes at a time.
    body: f64,
    /// Text read individually and expected to be legible alone — diagnostics,
    /// the tree, the active status segments, a matched bracket.
    content: f64,
    /// Deliberately recessive text that must still resolve — line numbers, the
    /// inactive half of the status bar.
    quiet: f64,
}

impl Floors {
    fn for_ground(kind: Kind) -> Self {
        match kind {
            Kind::Dark => Floors {
                body: 11.5,
                content: 6.5,
                quiet: 5.0,
            },
            Kind::Light => Floors {
                body: 5.4,
                content: 2.6,
                quiet: 2.0,
            },
        }
    }

    fn ground(kind: Kind) -> &'static str {
        match kind {
            Kind::Dark => "dark",
            Kind::Light => "light",
        }
    }
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
    let floors = Floors::for_ground(kind);
    let ground = Floors::ground(kind);

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
    at_least(
        &mut bad,
        "fg on bg",
        theme.fg,
        theme.bg,
        floors.body,
        ground,
    );

    at_least(
        &mut bad,
        "selection_fg on selection_bg",
        theme.selection_fg,
        theme.selection_bg,
        floors.content,
        ground,
    );
    at_least(
        &mut bad,
        "selection_fg on selection_primary_bg",
        theme.selection_fg,
        theme.selection_primary_bg,
        floors.content,
        ground,
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
    separated_by(
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
        floors.quiet,
        ground,
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
    at_least(
        &mut bad,
        "fg on cursor_line_bg",
        theme.fg,
        theme.cursor_line_bg,
        floors.body,
        ground,
    );

    at_least(
        &mut bad,
        "bracket_match_fg on bracket_match_bg",
        theme.bracket_match_fg,
        theme.bracket_match_bg,
        floors.content,
        ground,
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
        floors.content,
        ground,
    );
    // Inactive is quieter but still legible: it carries real content — the
    // filetype, the line ending — not decoration.
    at_least(
        &mut bad,
        "status_bar_inactive_fg",
        theme.status_bar_inactive_fg,
        theme.status_bar_bg,
        floors.quiet,
        ground,
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
    // Against `chrome_bg`, not `bg`: the sidebar draws on the raised surface,
    // and an audit checking the wrong background is worse than no audit — it
    // reports a ratio nobody ever sees.
    at_least(
        &mut bad,
        "tree_directory_fg on chrome_bg",
        theme.tree_directory_fg,
        theme.chrome_bg,
        floors.content,
        ground,
    );
    at_least(
        &mut bad,
        "tree_file_fg on chrome_bg",
        theme.tree_file_fg,
        theme.chrome_bg,
        floors.content,
        ground,
    );
    // And the surface has to actually be a surface. Chrome and content sharing
    // one colour is the defect this field exists to fix, and a theme that sets
    // them equal has silently undone it.
    differ(
        &mut bad,
        "chrome_bg vs bg",
        theme.chrome_bg,
        theme.bg,
        "chrome and content have to be tellable apart",
    );

    for (name, colour) in [
        ("diagnostic_error", theme.diagnostic_error),
        ("diagnostic_warning", theme.diagnostic_warning),
        ("diagnostic_info", theme.diagnostic_info),
        ("diagnostic_hint", theme.diagnostic_hint),
    ] {
        at_least(&mut bad, name, colour, theme.bg, floors.content, ground);
    }

    // Red and amber are the classic deuteranopia collision, and error-versus-
    // warning is the one diagnostic distinction that changes what a user does.
    // Separating them by lightness as well as hue is what keeps that decision
    // available to a red-green colour-blind reader. Palettes designed for
    // harmony fail this and have to be adapted — Rosé Pine misses it by 0.03.
    separated_by(
        &mut bad,
        "diagnostic_error vs diagnostic_warning",
        theme.diagnostic_error,
        theme.diagnostic_warning,
        1.8,
    );

    bad
}

/// Text against the surface behind it, measured against the floor its ground
/// asks for.
///
/// The message names the ground, because a floor that moves with `kind` is
/// otherwise a number the reader cannot check.
fn at_least(bad: &mut Vec<String>, name: &str, a: Color, b: Color, floor: f64, ground: &str) {
    let ratio = contrast(a, b);
    if ratio < floor {
        bad.push(format!(
            "{name}: contrast {ratio:.2} is below the {floor:.1} floor for a {ground} ground"
        ));
    }
}

/// Two colours that have to be tellable apart, with no text between them.
///
/// Deliberately *not* [`at_least`]: a surface against another surface, or a red
/// against an amber, is not text on a background, and the ground-dependent
/// floors are a correction for how text legibility behaves. Applying them here
/// would be measuring one thing with another thing's ruler.
fn separated_by(bad: &mut Vec<String>, name: &str, a: Color, b: Color, floor: f64) {
    let ratio = contrast(a, b);
    if ratio < floor {
        bad.push(format!(
            "{name}: contrast {ratio:.2} is below the {floor:.1} separation"
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
