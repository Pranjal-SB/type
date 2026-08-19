//! What a theme has to be true of, checked rather than asserted.
//!
//! A palette is the one part of an editor where "looks fine to me" is the usual
//! standard of proof, and it is the wrong one: legibility is a measurable
//! property and colour-blind users are not served by a designer's eye. These
//! tests compute WCAG contrast from the actual channel values, so a palette
//! change that makes text unreadable fails a build rather than shipping.
//!
//! The rubric itself is `typ_core::audit`, not a private helper here. Three
//! callers need it and only two are tests: the 256-colour degradation is checked
//! against the same rules, every shipped theme file is checked at both depths
//! from another crate, and a theme author writing their own file needs the
//! answer the project holds itself to. Three copies of a rubric become three
//! rubrics the first time one is edited.

use ratatui::style::Color;
use typ_core::ThemeColors;

use typ_core::audit;
use typ_core::theme::Kind;

#[track_caller]
fn assert_clean(name: &str, theme: &ThemeColors, kind: Kind) {
    let bad = audit(theme, kind);
    assert!(
        bad.is_empty(),
        "{name} fails {} of the palette rules:
  {}",
        bad.len(),
        bad.join(
            "
  "
        )
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
fn the_shipped_palette_satisfies_every_rule() {
    assert_clean("the default theme", &ThemeColors::default(), Kind::Dark);
}

#[test]
fn a_correct_light_palette_satisfies_every_rule() {
    // The regression this file exists to prevent: four of the rules above were
    // written as luminance comparisons and rejected this palette outright.
    assert_clean("the light fixture", &light_fixture(), Kind::Light);
}

#[test]
fn the_rules_reject_a_palette_that_earns_it() {
    // The audit is only worth running if it can fail. Body text the same colour
    // as the page is the least arguable way to earn that.
    let mut theme = ThemeColors::default();
    theme.fg = theme.bg;

    let bad = audit(&theme, Kind::Dark);

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
    let failed_on_cursor_line = |theme: &ThemeColors, kind: Kind| {
        audit(theme, kind)
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
        !failed_on_cursor_line(&pale, Kind::Light),
        "6.76 clears the 6.5 floor a pale ground gets"
    );

    let dark = ThemeColors {
        fg: Color::Rgb(0x9e, 0xa4, 0xac),
        cursor_line_bg: Color::Rgb(0x17, 0x1c, 0x25), // 6.80 against fg
        ..ThemeColors::default()
    };

    assert!(
        failed_on_cursor_line(&dark, Kind::Dark),
        "6.80 misses the 7.0 floor a dark ground keeps — a dark palette has \
         somewhere else to go, so it does not get the exemption"
    );
}

// ---------------------------------------------------------------------------
// The same rules, after the palette has been quantised
// ---------------------------------------------------------------------------

#[test]
fn a_colour_lands_on_the_nearest_thing_the_cube_can_say() {
    // The page background, which is the hardest class of colour to quantise:
    // near-black, slightly blue, and sitting in the part of RGB space where
    // equal numeric steps are least equal perceptually.
    assert_eq!(
        typ_core::downgrade(Color::Rgb(0x10, 0x14, 0x1b), typ_core::Depth::Ansi256),
        Color::Rgb(0x12, 0x12, 0x12),
        "index 233 on the grey ramp is the closest the 256-colour set gets"
    );
}

#[test]
fn truecolor_leaves_a_palette_exactly_as_written() {
    let theme = ThemeColors::default();
    assert_eq!(
        typ_core::colour::downgrade_theme(&theme, typ_core::Depth::TrueColor),
        theme
    );
}

#[test]
fn the_shipped_palette_still_reads_at_256_colours() {
    // Nobody in the field checks this, so nobody knows whether their theme is
    // legible on a 256-colour terminal. The rubric is the same; only the
    // palette has changed underneath it.
    let degraded =
        typ_core::colour::downgrade_theme(&ThemeColors::default(), typ_core::Depth::Ansi256);
    assert_clean("the default theme at 256 colours", &degraded, Kind::Dark);
}

#[test]
fn a_palette_that_lies_about_its_ground_is_reported() {
    // The declared kind picks one of the floors, so a theme that says "dark"
    // over a pale page would be audited against the wrong one — silently, which
    // is the worst way for a rule to be wrong.
    let bad = audit(&light_fixture(), Kind::Dark);

    assert!(
        bad.iter().any(|f| f.starts_with("kind is")),
        "a light palette declared dark has to be caught, got: {bad:?}"
    );
}

#[test]
fn a_colour_the_terminal_owns_is_not_a_colour_a_theme_can_pick() {
    // Below truecolor TYPE inherits whatever the user's terminal defines each
    // slot as. That cannot be measured from here, so it cannot be audited, so
    // it cannot be shipped in a palette.
    let theme = ThemeColors {
        fg: Color::Blue,
        ..ThemeColors::default()
    };

    let bad = audit(&theme, Kind::Dark);

    assert!(
        bad.iter().any(|f| f.contains("not a truecolor value")),
        "got: {bad:?}"
    );
}
