use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{KeyChord, Panel, PanelEvent};
use typ_panel_tree::TreePanel;

/// One directory per test: cargo runs tests in threads, and a shared fixture
/// path would have each test deleting the tree another is reading.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tree-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("a.rs"), "").unwrap();
    std::fs::write(dir.join("b.rs"), "").unwrap();
    std::fs::write(dir.join("sub/c.rs"), "").unwrap();
    dir
}

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn lists_entries_in_the_root_directory() {
    let t = TreePanel::new(&fixture("list")).unwrap();
    // sub/, a.rs, b.rs — directories sort first.
    assert_eq!(t.entry_count(), 3);
}

#[test]
fn directories_sort_before_files() {
    let t = TreePanel::new(&fixture("sort")).unwrap();
    assert!(t.selected().unwrap().is_dir());
}

#[test]
fn arrow_keys_move_the_selection() {
    let mut t = TreePanel::new(&fixture("arrows")).unwrap();
    t.handle_key(chord(KeyCode::Down));
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "a.rs");
}

#[test]
fn selection_clamps_at_the_end_of_the_list() {
    let mut t = TreePanel::new(&fixture("clamp")).unwrap();
    for _ in 0..50 {
        t.handle_key(chord(KeyCode::Down));
    }
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "b.rs");
}

#[test]
fn pressing_enter_on_a_file_emits_open_file() {
    let mut t = TreePanel::new(&fixture("enter-file")).unwrap();
    t.handle_key(chord(KeyCode::Down)); // a.rs
    let events = t.handle_key(chord(KeyCode::Enter));
    assert!(matches!(
        events.first(),
        Some(PanelEvent::OpenFile {
            line: 0,
            col: 0,
            ..
        })
    ));
}

#[test]
fn pressing_enter_on_a_directory_does_not_emit_open_file() {
    let mut t = TreePanel::new(&fixture("enter-dir")).unwrap();
    let events = t.handle_key(chord(KeyCode::Enter));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, PanelEvent::OpenFile { .. }))
    );
}

#[test]
fn the_sidebar_sits_on_the_chrome_surface_not_the_editors_page() {
    // The tree, the gutter and the editor were all `bg`: three regions in one
    // colour, which is why no amount of border made them read as separate
    // things. Chrome is raised, content is the floor.
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use typ_core::{RenderContext, ThemeColors};

    let theme = ThemeColors::default();
    let mut panel = TreePanel::new(&fixture("surface")).unwrap();
    let area = Rect::new(0, 0, 20, 6);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics: &[],
        is_focused: true,
        panel_index: 0,
        terminal_width: 20,
        terminal_height: 6,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);

    assert_ne!(
        theme.chrome_bg, theme.bg,
        "a sidebar the same colour as the page is the thing being fixed"
    );
    // The border row, an unselected entry, and the empty rows below them. One
    // row left on `bg` is a band across the sidebar.
    //
    // Row 1 is skipped on purpose: it is the selected entry and carries
    // `selection_primary_bg`, which is the tree saying where the cursor is and
    // has nothing to do with the surface underneath it.
    for y in [0, 2, 3, 4, 5] {
        for x in 0..20 {
            assert_eq!(
                buf[(x, y)].bg,
                theme.chrome_bg,
                "cell {x},{y} is not on the chrome surface"
            );
        }
    }
    assert_eq!(
        buf[(1, 1)].bg,
        theme.selection_primary_bg,
        "and the selected row still marks itself"
    );
}
