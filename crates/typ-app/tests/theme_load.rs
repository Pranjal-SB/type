//! Choosing a theme, finding it, and bringing it down to what the terminal can
//! show.

use std::path::{Path, PathBuf};

use ratatui::style::Color;
use typ_app::config::{load_settings, load_theme};
use typ_core::{Depth, ThemeColors};
use typ_panel_editor::render::Whitespace;

/// A config directory of this test's own.
///
/// Created, never removed first. On Windows `remove_dir_all` can return before
/// the directory is actually gone, so removing and recreating fails
/// intermittently — the same trap `typ/src/main.rs` documents. Names are unique
/// per test, so there is nothing stale to clear.
fn config_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-theme-test").join(name);
    std::fs::create_dir_all(dir.join("themes")).unwrap();
    dir
}

fn write_theme(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join("themes").join(format!("{name}.toml")), contents).unwrap();
}

#[test]
fn the_shipped_default_loads_from_its_own_file() {
    // The point of the exercise: the default is a theme file read through the
    // same loader as every other theme, not a private path. A default that
    // takes a shortcut is a default that drifts from the format.
    let (colors, _syntax, warning) = load_theme(None, "slate", Depth::TrueColor);

    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(colors, ThemeColors::default());
}

#[test]
fn an_unknown_theme_name_warns_and_keeps_the_shipped_palette() {
    let (colors, _syntax, warning) = load_theme(None, "nonesuch", Depth::TrueColor);

    let warning = warning.expect("an unknown theme has to be reported");
    assert!(warning.contains("nonesuch"), "warning: {warning}");
    assert_eq!(
        colors,
        ThemeColors::default(),
        "an editor that will not start because of a theme name is an editor \
         you cannot use to fix the theme name"
    );
}

#[test]
fn a_file_in_the_config_directory_wins_over_the_embedded_one() {
    // What makes "copy a shipped theme and edit it" work, and the only reason
    // the embedded set is not a closed list.
    let dir = config_dir("override");
    write_theme(
        &dir,
        "slate",
        "name = \"Mine\"\nkind = \"dark\"\n\n[ui]\nfg = \"#ff0000\"\n",
    );

    let (colors, _syntax, warning) = load_theme(Some(&dir), "slate", Depth::TrueColor);

    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(colors.fg, Color::Rgb(0xff, 0x00, 0x00));
}

#[test]
fn a_theme_name_with_no_file_anywhere_falls_back_to_the_embedded_set() {
    let dir = config_dir("no-override");

    let (colors, _syntax, warning) = load_theme(Some(&dir), "slate", Depth::TrueColor);

    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(colors, ThemeColors::default());
}

#[test]
fn a_broken_user_theme_warns_and_keeps_the_shipped_palette() {
    let dir = config_dir("broken");
    write_theme(
        &dir,
        "slate",
        "name = \"Broken\"\nkind = \"dark\"\n\n[ui]\nfg = \"not a colour\"\n",
    );

    let (colors, _syntax, warning) = load_theme(Some(&dir), "slate", Depth::TrueColor);

    let warning = warning.expect("a broken theme has to be reported");
    assert!(warning.contains("slate.toml"), "which file: {warning}");
    assert_eq!(colors, ThemeColors::default());
}

#[test]
fn the_palette_arrives_already_degraded() {
    // Nothing downstream branches on colour depth. render.rs, gutter.rs and
    // status.rs take a ThemeColors and stay unaware that depth is a thing,
    // which is only true if the quantising happens here.
    let (colors, _syntax, _) = load_theme(None, "slate", Depth::Ansi256);

    assert_ne!(colors.bg, ThemeColors::default().bg);
    assert_eq!(
        colors,
        typ_core::colour::downgrade_theme(&ThemeColors::default(), Depth::Ansi256)
    );
}

// ---------------------------------------------------------------------------
// config.toml
// ---------------------------------------------------------------------------

fn write_settings(name: &str, contents: &str) -> PathBuf {
    let dir = config_dir(name);
    let path = dir.join("config.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn no_settings_file_yields_the_defaults_and_no_complaint() {
    let (settings, warning) = load_settings(None);

    assert!(warning.is_none());
    assert_eq!(settings.theme, "slate");
    assert_eq!(settings.color_depth, None, "None means detect it");
}

#[test]
fn a_missing_settings_file_is_not_an_error() {
    let path = PathBuf::from("does/not/exist/config.toml");
    let (_, warning) = load_settings(Some(&path));
    assert!(warning.is_none(), "an absent config is the normal case");
}

#[test]
fn the_theme_name_comes_from_the_settings_file() {
    let path = write_settings("theme-name", "theme = \"mocha\"\n");
    let (settings, warning) = load_settings(Some(&path));

    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(settings.theme, "mocha");
}

#[test]
fn colour_depth_can_be_forced() {
    // The escape hatch detection cannot provide: nothing in the environment
    // separates a tmux that forwards truecolor from one that mangles it, so
    // the answer is a setting rather than a better guess.
    let path = write_settings("depth", "color_depth = \"256\"\n");
    let (settings, warning) = load_settings(Some(&path));

    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(settings.color_depth, Some(Depth::Ansi256));
}

#[test]
fn an_unknown_colour_depth_warns_and_leaves_it_to_detection() {
    let path = write_settings("bad-depth", "color_depth = \"thousands\"\n");
    let (settings, warning) = load_settings(Some(&path));

    let warning = warning.expect("a bad depth has to be reported");
    assert!(warning.contains("thousands"), "warning: {warning}");
    assert_eq!(settings.color_depth, None);
}

#[test]
fn the_indent_width_can_be_stated_instead_of_measured() {
    // Detection is a heuristic and a file that mixes units can defeat it. The
    // user needs somewhere to say so that is not "edit the file until the
    // heuristic agrees".
    let path = write_settings("indent", "indent_width = 2\n");
    let (settings, warning) = load_settings(Some(&path));

    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(settings.indent_width, Some(2));
}

#[test]
fn no_indent_width_leaves_it_to_the_file() {
    let (settings, _) = load_settings(None);
    assert_eq!(settings.indent_width, None, "None means measure it");
}

#[test]
fn an_indent_width_of_zero_warns_and_leaves_it_to_detection() {
    // Zero is a width no editor can insert, and silently treating it as one
    // would be a setting that appears to take effect and does not.
    let path = write_settings("indent-zero", "indent_width = 0\n");
    let (settings, warning) = load_settings(Some(&path));

    let warning = warning.expect("a zero width has to be reported");
    assert!(warning.contains("indent_width"), "warning: {warning}");
    assert_eq!(settings.indent_width, None);
}

#[test]
fn whitespace_defaults_to_marking_only_what_is_selected() {
    // VS Code's default, and the right one: whitespace is diagnostic inside a
    // selection and clutter everywhere else.
    let (settings, _) = load_settings(None);
    assert_eq!(settings.whitespace, Whitespace::Selection);
}

#[test]
fn each_whitespace_value_parses() {
    for (text, expected) in [
        ("none", Whitespace::None),
        ("trailing", Whitespace::Trailing),
        ("selection", Whitespace::Selection),
        ("all", Whitespace::All),
    ] {
        let path = write_settings(&format!("ws-{text}"), &format!("whitespace = {text:?}\n"));
        let (settings, warning) = load_settings(Some(&path));
        assert!(warning.is_none(), "{text}: {warning:?}");
        assert_eq!(settings.whitespace, expected, "{text}");
    }
}

#[test]
fn an_unknown_whitespace_value_warns_and_keeps_the_default() {
    let path = write_settings("ws-bad", "whitespace = \"boundary\"\n");
    let (settings, warning) = load_settings(Some(&path));

    // `boundary` is VS Code's fifth value and the one deliberately not taken,
    // so it is exactly the mistake somebody arriving from there will make.
    let warning = warning.expect("an unknown value has to be reported");
    assert!(warning.contains("whitespace"), "warning: {warning}");
    assert_eq!(settings.whitespace, Whitespace::Selection);
}

#[test]
fn an_unknown_setting_warns_and_keeps_the_rest() {
    // Unlike a theme file, a stray key here is survivable: the settings that
    // parsed are still the settings the user asked for, and refusing all of
    // them over one typo helps nobody.
    let path = write_settings("unknown", "theme = \"mocha\"\nwombat = true\n");
    let (settings, warning) = load_settings(Some(&path));

    let warning = warning.expect("an unknown key has to be reported");
    assert!(warning.contains("wombat"), "warning: {warning}");
    assert_eq!(settings.theme, "mocha");
}
