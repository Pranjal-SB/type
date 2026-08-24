use std::sync::Arc;

use typ_panel_editor::EditorPanel;
use typ_syntax::{Language, Syntax};

#[test]
fn a_rust_file_gets_a_language() {
    let panel = EditorPanel::new_at(std::path::Path::new("main.rs"));
    assert_eq!(panel.language(), Some(Language::Rust));
}

#[test]
fn a_file_with_no_grammar_gets_none() {
    let panel = EditorPanel::new_at(std::path::Path::new("notes.txt"));
    assert_eq!(panel.language(), None);
}

#[test]
fn a_scratch_buffer_with_no_path_gets_none() {
    // `App::new` starts here, before any file is opened. No path means no
    // extension means no grammar, and that is a normal state rather than a
    // degraded one.
    let panel = EditorPanel::from_str("fn main() {}\n");
    assert_eq!(panel.language(), None);
}

#[test]
fn an_older_generation_is_discarded() {
    // The counter earning its place. A result arriving after a newer one has
    // landed must not replace it, or a fast edit followed by a slow parse
    // paints the file as it was two keystrokes ago and stays that way.
    let mut panel = EditorPanel::from_str("fn main() {}\n");
    let rope = ropey::Rope::from_str("fn main() {}\n");
    let new = Arc::new(Syntax::parse(Language::Rust, &rope).unwrap());
    let old = Arc::new(Syntax::parse(Language::Rust, &rope).unwrap());

    panel.set_syntax(5, new.clone());
    panel.set_syntax(3, old);
    assert!(Arc::ptr_eq(panel.syntax().unwrap(), &new));
}

#[test]
fn a_newer_generation_replaces_the_tree() {
    let mut panel = EditorPanel::from_str("fn main() {}\n");
    let rope = ropey::Rope::from_str("fn main() {}\n");
    let first = Arc::new(Syntax::parse(Language::Rust, &rope).unwrap());
    let second = Arc::new(Syntax::parse(Language::Rust, &rope).unwrap());

    panel.set_syntax(1, first);
    panel.set_syntax(2, second.clone());
    assert!(Arc::ptr_eq(panel.syntax().unwrap(), &second));
}

#[test]
fn a_snapshot_is_the_buffer_as_it_is_now() {
    // What the worker parses. Ropey's nodes are shared, so the clone is cheap
    // and diverges from the original only where one of them is edited.
    let panel = EditorPanel::from_str("fn main() {}\n");
    let snapshot = panel.buffer().snapshot();
    assert_eq!(snapshot.to_string(), "fn main() {}\n");
}

#[test]
fn a_file_past_the_size_guard_gets_no_language() {
    // Above the guard a file renders as plain text and asks for no parses.
    // Same state as a file with no grammar, reached by a different route, and
    // every consumer downstream already handles it.
    //
    // Built from a small line repeated rather than one enormous line: the
    // guard is about total bytes, and a rope of one 5 MB line would also be
    // testing ropey's chunking.
    let huge = "fn a() {}\n".repeat(500_000); // ~5 MB, past the 4 MB guard
    assert!(huge.len() > 4 * 1024 * 1024);

    let dir = std::env::temp_dir().join("typ-size-guard");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("huge.rs");
    std::fs::write(&path, &huge).unwrap();

    let panel = EditorPanel::from_path(&path).unwrap();
    assert_eq!(
        panel.language(),
        None,
        "a 5 MB .rs file was handed to the parser"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_file_under_the_size_guard_keeps_its_language() {
    // The other side of the threshold, so the guard cannot be satisfied by
    // switching highlighting off for everything.
    let panel = EditorPanel::new_at(std::path::Path::new("small.rs"));
    assert_eq!(panel.language(), Some(Language::Rust));
}
