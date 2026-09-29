//! The registry's answer is what decides the panel. Gap 116.

use std::path::PathBuf;

use typ_app::App;
use typ_core::{HandlerId, PanelEvent};

fn dir_with(name: &str, file: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-registry-wiring").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), bytes).unwrap();
    dir
}

#[test]
fn a_path_registered_to_another_viewer_is_not_opened_as_text() {
    // `handler_for` was computed and thrown away, and an `EditorPanel` built
    // regardless, so the first viewer anyone registered would open its files
    // as garbage text, with no error. Until that viewer exists, say so.
    let dir = dir_with("image", "logo.png", b"not really a png");
    let mut app = App::new(&dir).unwrap();
    app.registry_mut().register("png", HandlerId("image"));

    // The same event Enter in the tree and a picker click both produce.
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("logo.png"),
        at: typ_core::Position::default(),
    }])
    .unwrap();

    assert!(
        app.editor().path().is_none(),
        "a file registered to the image viewer opened in the text editor"
    );
    let status = app.status().unwrap_or_default().to_string();
    assert!(
        status.contains("logo.png") && status.contains("image"),
        "nothing said why it did not open: {status:?}"
    );
}

#[test]
fn a_path_the_editor_handles_still_opens() {
    let dir = dir_with("editor", "main.rs", b"fn main() {}\n");
    let mut app = App::new(&dir).unwrap();
    app.registry_mut().register("png", HandlerId("image"));

    app.open_path(&dir.join("main.rs")).unwrap();

    assert!(app.editor().path().is_some());
}
