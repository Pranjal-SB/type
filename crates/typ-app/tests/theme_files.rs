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
