//! The tab bar: where the cells land, and what gets drawn in them.
//!
//! Cell layout is a function of its own rather than something the renderer works
//! out inline, because Task 6 hit-tests against exactly these rectangles. The
//! gutter learned at M2.3 and the picker relearned at M2.8 that two call sites
//! doing the same arithmetic drift by a cell, and every click lands a column
//! from the pointer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::layout::split_tabs;
use typ_app::tabbar::{self, TabCell};

fn labels(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

/// Every symbol in one row of a buffer, joined.
fn row(buf: &Buffer, y: u16) -> String {
    (buf.area.x..buf.area.right())
        .map(|x| buf[(x, y)].symbol())
        .collect()
}

#[test]
fn one_tab_gets_no_bar() {
    // A strip naming the only open file says nothing the file's own border does
    // not already say, and costs a row of the buffer to say it.
    let editor = Rect::new(0, 0, 40, 10);
    let (bar, rest) = split_tabs(editor, 1);

    assert_eq!(bar.height, 0);
    assert_eq!(rest, editor, "the editor gave up a row for an empty bar");
}

#[test]
fn two_tabs_take_a_row_off_the_top_of_the_editor() {
    let editor = Rect::new(4, 2, 40, 10);
    let (bar, rest) = split_tabs(editor, 2);

    assert_eq!(bar, Rect::new(4, 2, 40, 1));
    assert_eq!(rest, Rect::new(4, 3, 40, 9));
    assert_eq!(
        bar.height + rest.height,
        editor.height,
        "a row went missing between the two"
    );
}

#[test]
fn an_editor_too_short_to_spare_a_row_keeps_all_of_them() {
    // Two rows is a border and nothing else. Taking one for tabs leaves a panel
    // that cannot show a single line of the file it is naming.
    let editor = Rect::new(0, 0, 40, 2);
    let (bar, rest) = split_tabs(editor, 3);

    assert_eq!(bar.height, 0);
    assert_eq!(rest, editor);
}

#[test]
fn cells_run_left_to_right_without_gaps_or_overlap() {
    let cells = tabbar::cells(&labels(&["a.rs", "b.rs", "c.rs"]), 0, 60);

    assert_eq!(cells.len(), 3);
    for pair in cells.windows(2) {
        assert_eq!(
            pair[0].x + pair[0].width,
            pair[1].x,
            "cells {pair:?} do not abut"
        );
    }
}

#[test]
fn a_cell_is_wide_enough_for_its_name() {
    let cells = tabbar::cells(&labels(&["main.rs"]), 0, 60);

    assert!(
        cells[0].width as usize > "main.rs".len(),
        "a cell exactly as wide as the name leaves the tabs touching: {cells:?}"
    );
}

#[test]
fn the_active_tab_is_always_among_the_visible_cells() {
    // The promise the bar exists to make. Twenty tabs will not fit in forty
    // columns, and the one that must be on screen is the one being edited.
    let names: Vec<String> = (0..20).map(|i| format!("file{i}.rs")).collect();

    for active in [0, 7, 13, 19] {
        let cells = tabbar::cells(&names, active, 40);
        assert!(
            cells.iter().any(|c| c.index == active && c.width > 0),
            "tab {active} scrolled off the bar: {cells:?}"
        );
    }
}

#[test]
fn cells_never_run_past_the_bar() {
    let names: Vec<String> = (0..20).map(|i| format!("file{i}.rs")).collect();
    let cells = tabbar::cells(&names, 19, 40);

    for cell in &cells {
        assert!(
            cell.x + cell.width <= 40,
            "cell {cell:?} runs past a 40-column bar"
        );
    }
}

#[test]
fn a_name_longer_than_the_whole_bar_is_clipped_rather_than_dropped() {
    let long = "a-very-long-generated-file-name-nobody-would-type.rs";
    let cells = tabbar::cells(&labels(&[long]), 0, 20);

    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].width, 20);
}

#[test]
fn a_zero_width_bar_lays_out_nothing() {
    assert!(tabbar::cells(&labels(&["a.rs"]), 0, 0).is_empty());
}

#[test]
fn the_bar_draws_every_visible_name() {
    let area = Rect::new(0, 0, 40, 1);
    let mut buf = Buffer::empty(area);
    let theme = typ_core::ThemeColors::default();

    tabbar::draw(&mut buf, area, &labels(&["main.rs", "lib.rs"]), 0, &theme);

    let drawn = row(&buf, 0);
    assert!(drawn.contains("main.rs"), "got {drawn:?}");
    assert!(drawn.contains("lib.rs"), "got {drawn:?}");
}

#[test]
fn the_active_tab_is_drawn_differently_from_the_others() {
    // Two names and no way to tell which one you are editing is a bar that
    // costs a row and answers nothing.
    let area = Rect::new(0, 0, 40, 1);
    let mut buf = Buffer::empty(area);
    let theme = typ_core::ThemeColors::default();

    tabbar::draw(&mut buf, area, &labels(&["main.rs", "lib.rs"]), 0, &theme);

    let cells = tabbar::cells(&labels(&["main.rs", "lib.rs"]), 0, 40);
    let active = &cells[0];
    let other = &cells[1];
    assert_ne!(
        buf[(active.x + 1, 0)].style(),
        buf[(other.x + 1, 0)].style(),
        "the active tab is painted exactly like the inactive one"
    );
}

#[test]
fn a_dirty_tab_is_marked() {
    // The label carries the marker, the same `*` the panel border uses, so
    // there is one spelling of "unsaved" in the editor rather than two.
    let area = Rect::new(0, 0, 40, 1);
    let mut buf = Buffer::empty(area);
    let theme = typ_core::ThemeColors::default();

    tabbar::draw(&mut buf, area, &labels(&["main.rs *", "lib.rs"]), 0, &theme);

    assert!(row(&buf, 0).contains("main.rs *"));
}

#[test]
fn drawing_into_a_bar_narrower_than_one_name_is_not_a_panic() {
    let area = Rect::new(0, 0, 3, 1);
    let mut buf = Buffer::empty(area);
    let theme = typ_core::ThemeColors::default();

    tabbar::draw(&mut buf, area, &labels(&["main.rs", "lib.rs"]), 1, &theme);
}

#[test]
fn a_wide_grapheme_is_measured_by_its_columns_not_its_count() {
    // A CJK filename is two columns per character. Counting graphemes would lay
    // the next cell down on top of this one's second half.
    let cells = tabbar::cells(&labels(&["日本語.rs", "b.rs"]), 0, 60);

    assert!(
        cells[0].width >= 9,
        "three double-width graphemes plus `.rs` and padding is at least 9 columns: {cells:?}"
    );
    assert_eq!(cells[0].x + cells[0].width, cells[1].x);
}

#[test]
fn the_editor_hit_test_starts_below_the_bar() {
    // Gap 3, and the only reason `split_tabs` is a function rather than two
    // lines inline. The bar pushes every coordinate in the editor down a row;
    // if `areas` does not know that, every click lands a line above the pointer.
    let dir = std::env::temp_dir().join("typ-tabbar-hittest");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();

    let frame = Rect::new(0, 0, 100, 30);
    let mut app = typ_app::App::new(&dir).unwrap();

    app.open_path(&dir.join("a.rs")).unwrap();
    let (_, one_tab) = app.areas(frame);
    assert_eq!(app.tab_bar_area(frame).height, 0, "one tab drew a bar");

    app.open_path(&dir.join("b.rs")).unwrap();
    let bar = app.tab_bar_area(frame);
    let (_, two_tabs) = app.areas(frame);

    assert_eq!(bar.height, 1);
    assert_eq!(
        two_tabs.y,
        bar.y + bar.height,
        "the editor does not start where the bar ends"
    );
    assert_eq!(
        two_tabs.y,
        one_tab.y + 1,
        "the editor did not move down when the bar appeared"
    );
    assert_eq!(two_tabs.height, one_tab.height - 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cell_knows_which_tab_it_is() {
    let names: Vec<String> = (0..20).map(|i| format!("file{i}.rs")).collect();
    let cells = tabbar::cells(&names, 19, 40);

    let first: &TabCell = &cells[0];
    assert!(
        first.index > 0,
        "the bar scrolled but the cells still claim to start at tab 0"
    );
}
