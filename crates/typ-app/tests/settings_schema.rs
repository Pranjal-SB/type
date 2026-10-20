//! The settings table (M4.4 Task 2): one row per setting, read by parsing,
//! the defaults and the settings tab alike.

use typ_app::config::schema::{self, Kind};
use typ_app::config::{Settings, load_settings};

#[test]
fn every_settings_field_has_a_row() {
    // Exhaustive: a field added to `Settings` stops this compiling until it is
    // listed here, and listing it fails the test until it has a row.
    let Settings {
        theme: _,
        color_depth: _,
        indent_width: _,
        whitespace: _,
        language_servers: _,
        layout: _,
        adapt: _,
        session: _,
    } = Settings::default();
    let keys = [
        "theme",
        "color_depth",
        "indent_width",
        "whitespace",
        "language",
        "layout",
        "adapt",
        "session",
    ];

    for key in keys {
        assert!(schema::row(key).is_some(), "no row for {key}");
    }
    assert_eq!(schema::ROWS.len(), keys.len(), "a row for no field");
}

#[test]
fn every_rows_default_is_what_settings_default_holds() {
    let defaults = Settings::default();
    assert!(!schema::ROWS.is_empty());
    for row in schema::ROWS {
        assert_eq!((row.show)(&defaults), row.default, "{}", row.key);
    }
}

#[test]
fn every_row_has_a_section_and_a_description() {
    for row in schema::ROWS {
        assert!(!row.section.is_empty(), "{}", row.key);
        assert!(!row.description.is_empty(), "{}", row.key);
    }
}

#[test]
fn a_choice_rows_default_is_one_of_its_choices() {
    for row in schema::ROWS {
        if let Kind::Choice(choices) = row.kind {
            assert!(choices.contains(&row.default), "{}", row.key);
        }
    }
}

#[test]
fn parsing_reads_through_the_table() {
    let dir = std::env::temp_dir().join("typ-settings-schema");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "theme = \"mocha\"\nindent_width = 2\nadapt = false\nwhitespace = \"all\"\n",
    )
    .unwrap();

    let (settings, warning) = load_settings(Some(&path));

    assert!(warning.is_none(), "{warning:?}");
    for (key, want) in [
        ("theme", "mocha"),
        ("indent_width", "2"),
        ("adapt", "false"),
        ("whitespace", "all"),
    ] {
        let row = schema::row(key).unwrap();
        assert_eq!((row.show)(&settings), want, "{key}");
    }
}

#[test]
fn an_unknown_key_still_warns() {
    let dir = std::env::temp_dir().join("typ-settings-schema-unknown");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, "colour = \"red\"\ntheme = \"latte\"\n").unwrap();

    let (settings, warning) = load_settings(Some(&path));

    assert_eq!(settings.theme, "latte");
    let warning = warning.expect("a warning");
    assert!(
        warning.contains("colour is not a setting this editor has"),
        "{warning}"
    );
}
