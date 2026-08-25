//! The app's half of the find worker: which results it keeps and which it drops.

use std::fs;
use std::path::PathBuf;

use typ_app::App;
use typ_core::AppEvent;
use typ_find::{FileHit, Found};

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-findwire-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture root");
        fs::write(dir.join("main.rs"), "fn main() {}").expect("fixture file");
        Fixture(dir)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn files(generation: u64, paths: &[&str]) -> Found {
    Found::Files {
        generation,
        hits: paths
            .iter()
            .map(|path| FileHit {
                path: path.to_string(),
                indices: Vec::new(),
            })
            .collect(),
    }
}

#[test]
fn a_found_event_converts_into_an_app_event() {
    // The `From` impl that lets `FindWorker::spawn` take the app's own sender,
    // without `typ-find` ever naming `AppEvent`.
    let event: AppEvent = files(1, &["a.rs"]).into();
    assert!(matches!(event, AppEvent::Found(_)));
}

#[test]
fn a_result_for_the_awaited_generation_is_kept() {
    let fixture = Fixture::new("kept");
    let mut app = App::new(&fixture.0).expect("app");

    let generation = app.request_filter("main".into(), 10);
    assert!(
        app.handle_found(files(generation, &["main.rs"])),
        "the awaited generation was dropped"
    );
    assert_eq!(app.find_hits().len(), 1);
}

#[test]
fn a_result_for_a_stale_generation_is_dropped() {
    // The M2.7 lesson, applied before it can bite: the worker's counter is
    // app-global and a result still in flight for the previous query must not
    // land in the list the user is currently looking at.
    let fixture = Fixture::new("stale");
    let mut app = App::new(&fixture.0).expect("app");

    let first = app.request_filter("m".into(), 10);
    let second = app.request_filter("ma".into(), 10);
    assert_ne!(first, second);

    assert!(
        !app.handle_found(files(first, &["wrong.rs"])),
        "a stale result was applied"
    );
    assert!(app.find_hits().is_empty(), "the stale hits were kept");

    assert!(app.handle_found(files(second, &["main.rs"])));
    assert_eq!(app.find_hits()[0].path, "main.rs");
}

#[test]
fn an_indexed_event_is_accepted_whatever_the_generation() {
    // `Indexed` answers no query, so it carries no generation to check. It must
    // not be filtered out by the staleness test that guards `Files`.
    let fixture = Fixture::new("indexed");
    let mut app = App::new(&fixture.0).expect("app");
    let _ = app.request_filter("x".into(), 10);

    assert!(app.handle_found(Found::Indexed { count: 7 }));
}
