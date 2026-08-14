use typ_buffer::{Position, Selection, Selections};

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

#[test]
fn a_caret_is_an_empty_selection() {
    let s = Selection::caret(pos(1, 4));
    assert!(s.is_empty());
    assert_eq!(s.anchor, s.head);
}

#[test]
fn range_returns_the_endpoints_in_document_order() {
    // Selected leftwards: the head is before the anchor.
    let s = Selection {
        anchor: pos(2, 5),
        head: pos(1, 0),
    };
    assert_eq!(s.range(), (pos(1, 0), pos(2, 5)));
}

#[test]
fn contains_is_half_open_so_touching_selections_do_not_overlap() {
    let s = Selection {
        anchor: pos(0, 2),
        head: pos(0, 5),
    };
    assert!(s.contains(pos(0, 2)));
    assert!(s.contains(pos(0, 4)));
    assert!(!s.contains(pos(0, 5)), "the end is exclusive");
}

#[test]
fn selections_always_hold_at_least_one() {
    let s = Selections::default();
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().head, pos(0, 0));
}

#[test]
fn the_primary_is_the_one_most_recently_added() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(5, 0)));
    assert_eq!(s.primary().head, pos(5, 0));
}

#[test]
fn selections_are_kept_in_document_order() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(9, 0)));
    s.push(Selection::caret(pos(4, 0)));
    let lines: Vec<usize> = s.iter().map(|sel| sel.head.line).collect();
    assert_eq!(lines, vec![0, 4, 9]);
}

#[test]
fn the_primary_survives_reordering() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(9, 0)));
    s.push(Selection::caret(pos(4, 0)));
    // Added last, so still primary even though it sorted into the middle.
    assert_eq!(s.primary().head, pos(4, 0));
}

#[test]
fn overlapping_selections_merge_into_one() {
    let mut s = Selections::default();
    s.set_single(Selection {
        anchor: pos(0, 0),
        head: pos(0, 6),
    });
    s.push(Selection {
        anchor: pos(0, 4),
        head: pos(0, 9),
    });
    assert_eq!(s.len(), 1);
    assert_eq!(s.iter().next().unwrap().range(), (pos(0, 0), pos(0, 9)));
}

#[test]
fn adjacent_selections_stay_separate() {
    let mut s = Selections::default();
    s.set_single(Selection {
        anchor: pos(0, 0),
        head: pos(0, 3),
    });
    s.push(Selection {
        anchor: pos(0, 3),
        head: pos(0, 6),
    });
    assert_eq!(s.len(), 2, "touching is not overlapping");
}

#[test]
fn collapse_to_heads_drops_everything_but_the_primary_caret() {
    let mut s = Selections::default();
    s.push(Selection {
        anchor: pos(2, 0),
        head: pos(2, 4),
    });
    s.collapse_to_heads();
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().head, pos(2, 4));
    assert!(s.primary().is_empty());
}

#[test]
fn map_in_place_rewrites_every_selection_then_restores_the_invariants() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(2, 0)));
    // Move everything to the same place; they must merge rather than pile up.
    s.map_in_place(|_| Selection::caret(pos(1, 1)));
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().head, pos(1, 1));
}

#[test]
fn two_carets_at_the_same_position_are_one_cursor() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(3, 3)));
    s.push(Selection::caret(pos(3, 3)));
    // Otherwise typing would insert twice in the same place.
    assert_eq!(s.len(), 2, "the caret at 0,0 and the one at 3,3");
    let heads: Vec<Position> = s.iter().map(|sel| sel.head).collect();
    assert_eq!(heads, vec![pos(0, 0), pos(3, 3)]);
}

#[test]
fn a_caret_inside_a_selection_is_absorbed_by_it() {
    let mut s = Selections::default();
    s.set_single(Selection {
        anchor: pos(0, 0),
        head: pos(0, 6),
    });
    s.push(Selection::caret(pos(0, 3)));
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().range(), (pos(0, 0), pos(0, 6)));
}
