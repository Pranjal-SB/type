//! Writing one setting into `config.toml` and keeping everything else (M4.4
//! Task 3). Every test writes in its own temp directory.

use std::path::PathBuf;

use typ_app::config::write_setting;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-settings-write").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const COMMENTED: &str = "\
# My settings. Keep this note.
layout = \"standard\"   # the dock, always

theme = \"slate\" # dark

# Servers below.
[[language]]
name = \"python\"
extensions = [\"py\"]
command = \"pylsp\"
";

#[test]
fn changing_one_value_changes_that_line_and_nothing_else() {
    let path = dir("one-line").join("config.toml");
    std::fs::write(&path, COMMENTED).unwrap();

    write_setting(&path, "theme", Some("mocha".into())).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after,
        COMMENTED.replace("theme = \"slate\" # dark", "theme = \"mocha\" # dark")
    );
}

#[test]
fn a_new_key_goes_in_beside_the_others_not_under_a_table() {
    let path = dir("new-key").join("config.toml");
    std::fs::write(&path, COMMENTED).unwrap();

    write_setting(&path, "adapt", Some(false.into())).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    let (top, _) = after.split_once("[[language]]").unwrap();
    assert!(top.contains("adapt = false"), "{after}");
    assert!(after.contains("# My settings. Keep this note."), "{after}");
    let (settings, warning) = typ_app::config::load_settings(Some(&path));
    assert!(warning.is_none(), "{warning:?}");
    assert!(!settings.adapt);
}

#[test]
fn none_removes_the_key() {
    let path = dir("remove").join("config.toml");
    std::fs::write(&path, COMMENTED).unwrap();

    write_setting(&path, "layout", None).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(!after.contains("layout"), "{after}");
    assert!(after.contains("theme = \"slate\" # dark"), "{after}");
}

#[test]
fn a_missing_file_is_created_with_just_that_key() {
    let path = dir("missing").join("nested").join("config.toml");

    write_setting(&path, "indent_width", Some(2i64.into())).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "indent_width = 2\n"
    );
}

#[test]
fn a_write_that_fails_leaves_the_old_file() {
    let dir = dir("fails");
    let path = dir.join("config.toml");
    std::fs::write(&path, COMMENTED).unwrap();
    // The temp file the write goes through cannot be created: a directory
    // stands where it would be.
    std::fs::create_dir_all(dir.join("config.toml.tmp")).unwrap();

    let result = write_setting(&path, "theme", Some("mocha".into()));

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), COMMENTED);
}

#[test]
fn a_file_that_does_not_parse_is_not_overwritten() {
    let path = dir("broken").join("config.toml");
    std::fs::write(&path, "theme = \"slate\nnot toml at all [[[\n").unwrap();

    let result = write_setting(&path, "theme", Some("mocha".into()));

    let error = result.expect_err("a broken file is the user's to fix");
    assert!(error.contains("config.toml"), "{error}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "theme = \"slate\nnot toml at all [[[\n"
    );
}
