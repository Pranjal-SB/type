//! The log is global state, so these run one at a time.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use typ_app::log::{self, Level};

static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner())
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-log-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.log"));
    let _ = std::fs::remove_file(&path);
    path
}

fn contents(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn logging_is_off_until_it_is_asked_for() {
    let _guard = exclusive();
    log::init(None);
    assert!(!log::is_enabled());
    // And writing while off is a no-op rather than a panic.
    log::write(Level::Info, "test", "should vanish");
}

#[test]
fn a_line_carries_a_time_a_level_a_module_and_a_message() {
    let _guard = exclusive();
    let path = temp("format");
    log::init(Some(&path));

    log::write(Level::Warn, "typ_app::config", "keys.toml is unreadable");
    log::init(None);

    let text = contents(&path);
    assert!(text.contains("WARN"), "no level: {text}");
    assert!(text.contains("typ_app::config"), "no module: {text}");
    assert!(
        text.contains("keys.toml is unreadable"),
        "no message: {text}"
    );
    // HH:MM:SS.mmm — two colons and a millisecond field.
    let stamp = text.split_whitespace().next().unwrap_or_default();
    assert_eq!(stamp.matches(':').count(), 2, "no timestamp: {text}");
    assert!(stamp.contains('.'), "no milliseconds: {text}");
}

#[test]
fn every_level_is_written() {
    let _guard = exclusive();
    let path = temp("levels");
    log::init(Some(&path));

    log::write(Level::Info, "m", "one");
    log::write(Level::Warn, "m", "two");
    log::write(Level::Error, "m", "three");
    log::init(None);

    let text = contents(&path);
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("INFO"));
    assert!(text.contains("ERROR"));
}

#[test]
fn opening_the_log_again_appends_rather_than_truncating() {
    let _guard = exclusive();
    let path = temp("append");

    log::init(Some(&path));
    log::write(Level::Info, "m", "first run");
    log::init(None);

    log::init(Some(&path));
    log::write(Level::Info, "m", "second run");
    log::init(None);

    let text = contents(&path);
    assert!(
        text.contains("first run") && text.contains("second run"),
        "the run that needs explaining is often the one before: {text}"
    );
}

#[test]
fn an_unwritable_path_disables_logging_instead_of_failing() {
    let _guard = exclusive();
    // A directory that does not exist, so the open cannot succeed.
    let path = std::env::temp_dir()
        .join("typ-log-test-missing-dir")
        .join("nested")
        .join("out.log");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());

    log::init(Some(&path));

    // A logger that takes the editor down with it is worse than no logger.
    assert!(!log::is_enabled());
    log::write(Level::Error, "m", "should vanish");
    log::init(None);
}

#[test]
fn the_env_var_names_the_file() {
    let _guard = exclusive();
    let path = temp("from-env");
    // SAFETY: single-threaded within this test, and the mutex keeps every other
    // test in this file out while it runs.
    unsafe { std::env::set_var("TYP_LOG", &path) };
    log::init_from_env();
    assert!(log::is_enabled());

    log::write(Level::Info, "m", "via the environment");
    log::init(None);
    unsafe { std::env::remove_var("TYP_LOG") };

    assert!(contents(&path).contains("via the environment"));
}

#[test]
fn no_env_var_means_no_log() {
    let _guard = exclusive();
    unsafe { std::env::remove_var("TYP_LOG") };
    log::init_from_env();
    assert!(!log::is_enabled());
}
