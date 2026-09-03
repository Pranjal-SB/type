//! Cursors and selections.
//!
//! There is no single-cursor type. A caret is an empty selection, and the
//! editor always holds a `Selections` — with one entry in the common case.
//! Adding multi-cursor later would mean rewriting every editing path twice:
//! once to add the concept, once to undo what the single-cursor assumption
//! baked in.

use crate::position::Position;

/// A range of text with a fixed `anchor` and a moving `head`.
///
/// The head is where the cursor is drawn and where typing happens. Extending
/// moves the head and leaves the anchor, which is what makes shift+arrow grow
/// and shrink from the end the user expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub fn caret(at: Position) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The endpoints in document order, regardless of which way it was made.
    pub fn range(&self) -> (Position, Position) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Half-open: the start is inside, the end is not.
    ///
    /// That is what makes two selections which merely touch — one ending where
    /// the next begins — stay separate instead of merging.
    pub fn contains(&self, pos: Position) -> bool {
        let (start, end) = self.range();
        pos >= start && pos < end
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::caret(Position::default())
    }
}

/// A non-empty, document-ordered, non-overlapping set of selections.
///
/// Every mutating method ends by restoring those invariants, so no editing
/// code has to defend against an out-of-order or overlapping set.
#[derive(Debug, Clone)]
pub struct Selections {
    list: Vec<Selection>,
    /// Index into `list`, retargeted after each sort so the selection the user
    /// is steering stays the one they added.
    primary: usize,
}

impl Default for Selections {
    fn default() -> Self {
        Self {
            list: vec![Selection::default()],
            primary: 0,
        }
    }
}

impl Selections {
    pub fn single(selection: Selection) -> Self {
        Self {
            list: vec![selection],
            primary: 0,
        }
    }

    /// Always at least 1 — the type's invariant.
    ///
    /// No `is_empty` to pair with it: it could only ever return false, and a
    /// method that is a constant invites callers to branch on something that
    /// never varies. The invariant is the API.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn primary(&self) -> Selection {
        self.list[self.primary]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.list.iter()
    }

    /// Replace everything with one selection.
    pub fn set_single(&mut self, selection: Selection) {
        self.list = vec![selection];
        self.primary = 0;
    }

    /// Add a selection and make it primary.
    pub fn push(&mut self, selection: Selection) {
        self.list.push(selection);
        self.primary = self.list.len() - 1;
        self.normalize();
    }

    /// Rewrite every selection, then restore the invariants.
    pub fn map_in_place(&mut self, mut f: impl FnMut(Selection) -> Selection) {
        for selection in &mut self.list {
            *selection = f(*selection);
        }
        self.normalize();
    }

    /// Drop every selection but the primary, and reduce it to its head.
    pub fn collapse_to_heads(&mut self) {
        let head = self.primary().head;
        self.set_single(Selection::caret(head));
    }

    fn normalize(&mut self) {
        let primary = self.list[self.primary];
        self.list.sort_by_key(|s| s.range());

        let mut merged: Vec<Selection> = Vec::with_capacity(self.list.len());
        for selection in self.list.drain(..) {
            match merged.last_mut() {
                Some(previous) if overlaps(*previous, selection) => {
                    *previous = union(*previous, selection);
                }
                _ => merged.push(selection),
            }
        }
        self.list = merged;

        // The primary may have been merged into a larger selection, so look for
        // whichever one now covers where it was rather than trusting an index.
        self.primary = self
            .list
            .iter()
            .position(|s| *s == primary || covers(*s, primary))
            .unwrap_or(0);
    }
}

fn overlaps(a: Selection, b: Selection) -> bool {
    let (_, a_end) = a.range();
    let (b_start, _) = b.range();
    if a_end > b_start {
        // Strictly greater, so selections that only touch stay separate — the
        // same rule as `Selection::contains` being half-open.
        return true;
    }
    // The boundary case, and it is one-sided. `a` is the earlier of the two
    // (`normalize` sorts by `range()` before it gets here) so `a_end == b_start`
    // means one of:
    //
    //   * `a` is a caret at `b`'s start. `[P,Q)` **does** contain `P`, so this
    //     is one cursor. Two carets at the same position are the same case.
    //   * `a` is a selection ending where `b` starts. Half-open, so they only
    //     touch: two cursors.
    //   * `a` is a selection and `b` a caret at its end. `[P,Q)` does **not**
    //     contain `Q`: two cursors.
    //
    // So it turns on `a` being empty and not on either being empty. Testing
    // both (which is what this did) left a caret sitting exactly at a
    // selection's start as a second entry covering the same position, and
    // `edit_at_each_selection` then replaced one grapheme to the right of what
    // was selected. Gap 72.
    a.is_empty() && a_end == b_start
}

/// The smallest selection covering both.
///
/// **Direction is part of the answer, not a detail of it.** The head is the end
/// that moves, so a merge that always returns a forward selection puts the head
/// at the wrong end of two leftward ones, and the next Shift+Left shrinks the
/// selection from the far edge instead of extending it from the near one.
///
/// Only agreement is honoured. Two selections travelling opposite ways have no
/// combined direction to preserve, and inventing one would mean guessing which
/// of them the user is about to move; forward is the older behaviour and is no
/// worse for that case. Gap 73.
fn union(a: Selection, b: Selection) -> Selection {
    let (a_start, a_end) = a.range();
    let (b_start, b_end) = b.range();
    let (start, end) = (a_start.min(b_start), a_end.max(b_end));

    let backwards = |s: &Selection| !s.is_empty() && s.head < s.anchor;
    if backwards(&a) && backwards(&b) {
        return Selection {
            anchor: end,
            head: start,
        };
    }
    Selection {
        anchor: start,
        head: end,
    }
}

fn covers(outer: Selection, inner: Selection) -> bool {
    let (o_start, o_end) = outer.range();
    let (i_start, i_end) = inner.range();
    o_start <= i_start && i_end <= o_end
}
