//! A server's `WorkspaceEdit`, applied the one way every feature applies it:
//! per document, through the editor, one undo step per buffer. A file that
//! is not open opens in a background tab and stays unsaved; a file operation
//! is refused whole rather than half applied.

use std::path::{Path, PathBuf};

use serde_json::json;
use typ_app::{App, Focus};
use typ_core::Action;

const A: &str = "fn foo() {}\nfoo();\n";
const B: &str = "use foo;\n";

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-workspace-edit").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), A).unwrap();
    std::fs::write(dir.join("b.rs"), B).unwrap();
    dir
}

fn uri(path: &Path) -> String {
    typ_lsp::path_to_uri(path).unwrap().as_str().to_string()
}

fn edit(line: u32, from: u32, to: u32, text: &str) -> serde_json::Value {
    json!({
        "range": {
            "start": { "line": line, "character": from },
            "end": { "line": line, "character": to },
        },
        "newText": text,
    })
}

/// `a.rs` open and focused, `b.rs` not open.
fn opened(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    app.set_focus(Focus::Editor);
    (app, dir)
}

#[test]
fn an_edit_over_an_open_and_an_unopened_file_changes_both_and_saves_neither() {
    let (mut app, dir) = opened("two-files");
    let (a, b) = (dir.join("a.rs"), dir.join("b.rs"));
    let change = json!({
        "changes": {
            uri(&a): [edit(0, 3, 6, "bar"), edit(1, 0, 3, "bar")],
            uri(&b): [edit(0, 4, 7, "bar")],
        }
    });
    app.apply_workspace_edit_for_test(&change);

    assert_eq!(app.editor().line_text(0), "fn bar() {}");
    assert_eq!(app.editor().line_text(1), "bar();");
    assert_eq!(app.active_tab(), 0, "the edit took the user to another tab");
    assert_eq!(app.tab_count(), 2, "b.rs did not open");
    assert_eq!(app.tab(1).path(), Some(b.as_path()));
    assert_eq!(app.tab(1).line_text(0), "use bar;");
    assert!(app.editor().is_dirty() && app.tab(1).is_dirty());
    assert_eq!(std::fs::read_to_string(&a).unwrap(), A, "a.rs was written");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), B, "b.rs was written");

    app.apply_named_action(Action::Undo).unwrap();
    assert_eq!(app.editor().line_text(0), "fn foo() {}", "not one undo");
    assert_eq!(app.editor().line_text(1), "foo();", "not one undo");
    app.activate_tab(1);
    app.apply_named_action(Action::Undo).unwrap();
    assert_eq!(app.editor().line_text(0), "use foo;");
}

#[test]
fn document_changes_are_applied_as_changes_are() {
    let (mut app, dir) = opened("document-changes");
    let a = dir.join("a.rs");
    let change = json!({
        "documentChanges": [{
            "textDocument": { "uri": uri(&a), "version": null },
            "edits": [edit(0, 3, 6, "baz")],
        }]
    });
    app.apply_workspace_edit_for_test(&change);
    assert_eq!(app.editor().line_text(0), "fn baz() {}");
}

#[test]
fn a_file_operation_is_refused_and_nothing_changes() {
    let (mut app, dir) = opened("file-op");
    let a = dir.join("a.rs");
    let change = json!({
        "documentChanges": [
            {
                "textDocument": { "uri": uri(&a), "version": null },
                "edits": [edit(0, 3, 6, "bar")],
            },
            { "kind": "create", "uri": uri(&dir.join("c.rs")) },
        ]
    });
    app.apply_workspace_edit_for_test(&change);
    assert_eq!(app.editor().line_text(0), "fn foo() {}", "half applied");
    assert!(!app.editor().is_dirty());
    assert!(!dir.join("c.rs").exists());
    let said = app.status().unwrap_or_default().to_string();
    assert!(said.contains("files"), "nothing said why: {said:?}");
}

#[test]
fn the_caret_after_an_edit_stays_on_its_word() {
    let (mut app, dir) = opened("caret");
    let a = dir.join("a.rs");
    app.editor_mut().goto(1, 4);
    let change = json!({ "changes": { uri(&a): [edit(1, 0, 3, "longer")] } });
    app.apply_workspace_edit_for_test(&change);
    assert_eq!(app.editor().line_text(1), "longer();");
    let caret = app.editor().cursor();
    assert_eq!((caret.line, caret.col), (1, 7));
}
