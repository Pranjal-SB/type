use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use typ_buffer::{
    EditKind, LineEnding, Position, SearchQuery, Selection, Selections, TextBuffer,
    display_to_grapheme_col, grapheme_to_display_col,
};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};
use typ_syntax::{Language, Syntax};

pub mod actions;
pub mod gutter;
mod occurrence;
pub mod render;

use crate::gutter::Gutter;
use crate::render::Whitespace;

/// The width to use when the file will not say.
///
/// Not public any more, and not read by anything outside this crate: what the
/// status bar states on screen is `EditorPanel::tab_width`, which is what the
/// editor is actually using. A constant readable from outside is a constant
/// somebody displays instead of the measurement.
const FALLBACK_TAB_WIDTH: usize = 4;

/// Lines the indent scan reads before it settles for what it has.
///
/// VS Code's number. Detection runs once per file open and a cold start has a
/// 100 ms budget, so the cap is what keeps opening a generated 400k-line file
/// from costing the same as opening a source file.
const INDENT_SCAN_LINES: usize = 10_000;

/// Lines beyond the viewport a bracket search may walk before giving up.
///
/// A partner just off-screen is worth finding — scrolling one line should not
/// make a highlight appear from nothing. A partner four hundred lines away is
/// not: nobody is reading both ends at once, and the scan would be on the
/// keystroke path. See `typ-buffer/src/brackets.rs`.
const BRACKET_SEARCH_MARGIN: usize = 64;

/// Lines the blank-line indent-guide scan reads, in each direction.
///
/// A blank line takes its guides from the shallower of the two non-blank lines
/// around it — without that, every empty line inside a block punches a hole
/// through the guides, which is most blocks. Finding those two lines is the one
/// thing in the render path that is not local to a single row, so it is
/// bounded: an unbounded walk to the next non-blank line is a walk over the
/// whole buffer on the keystroke path, which is the trap `line_text` already
/// taught this codebase once.
///
/// A gap longer than this draws no guide. That is a cosmetic miss rather than a
/// wrong answer, which is the whole reason the bound is acceptable here and was
/// not acceptable for the active-guide highlight Zed draws — a guide at the
/// wrong depth is a lie, and that one is cut instead.
const BLANK_GUIDE_SCAN: usize = 64;

pub struct EditorPanel {
    pub(crate) buffer: TextBuffer,
    /// Never a bare cursor: a caret is an empty selection, so every editing
    /// path is written once and works for one cursor or thirty.
    pub(crate) selections: Selections,
    pub(crate) top_line: usize,
    /// Leftmost *display* column drawn. Display, not grapheme: a line of CJK
    /// scrolls by cells the way it is drawn, not by characters.
    pub(crate) left_col: usize,
    /// Display column the cursor "wants", preserved across vertical movement
    /// so passing through short lines does not permanently lose the column.
    pub(crate) goal_col: Option<usize>,
    pub(crate) height: usize,
    /// Learned at render time, beside `height`: a panel does not know its size
    /// until it is asked to draw.
    pub(crate) width: usize,
    /// Where the current drag began, so a drag extends from the press rather
    /// than from wherever the cursor happened to be.
    drag_anchor: Option<Position>,
    /// The last cell clicked, so a second click in the same place can mean
    /// "select the word" without a double-click timer.
    last_click: Option<Position>,
    /// The gutter. Owned by the panel because its width is a function of this
    /// buffer's line count, and that width narrows the text area.
    pub(crate) gutter: Gutter,
    /// Columns a tab occupies, and the width one level of indent inserts.
    ///
    /// Measured from the buffer at load and settled there: re-measuring as the
    /// user types would let deleting a line change what Tab does.
    pub(crate) tab_width: usize,
    /// Which whitespace gets a visible mark — `whitespace` in `config.toml`.
    pub(crate) whitespace: Whitespace,
    /// The language, from the path's extension, settled at load like
    /// `tab_width` is. `None` when no grammar claims it, which is a normal
    /// state and not a degraded one.
    language: Option<Language>,
    /// The newest completed parse. `None` until the first one lands, which is
    /// the state every buffer is in for its first frame.
    syntax: Option<Arc<Syntax>>,
    /// The generation `syntax` came from, so a late result is dropped rather
    /// than applied.
    syntax_generation: u64,
}

impl EditorPanel {
    // Mirrors TextBuffer::from_str: infallible construction, so the FromStr
    // trait's Result shape would misrepresent it.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self::new(TextBuffer::from_str(s))
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        Ok(Self::new(TextBuffer::from_path(path)?))
    }

    /// An empty editor over a file that does not exist yet.
    pub fn new_at(path: &Path) -> Self {
        Self::new(TextBuffer::new_at(path))
    }

    fn new(buffer: TextBuffer) -> Self {
        let tab_width = typ_buffer::detect_indent_width(buffer.lines_str(INDENT_SCAN_LINES))
            .unwrap_or(FALLBACK_TAB_WIDTH);
        // Settled here rather than at each constructor because all three
        // funnel through this one, and a language that depended on which
        // constructor was used would be a bug nobody could see.
        let language = buffer
            .path()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .and_then(Language::for_extension);
        Self {
            tab_width,
            language,
            syntax: None,
            syntax_generation: 0,
            whitespace: Whitespace::default(),
            selections: Selections::default(),
            top_line: 0,
            left_col: 0,
            goal_col: None,
            height: 0,
            width: 0,
            drag_anchor: None,
            last_click: None,
            gutter: Gutter::default(),
            buffer,
        }
    }

    /// The indent width in force, measured from the file unless overridden.
    pub fn tab_width(&self) -> usize {
        self.tab_width
    }

    /// Override the measurement — `indent_width` in `config.toml`.
    ///
    /// A heuristic can be wrong on a file that mixes units, and the user needs
    /// somewhere to say so that is not "edit the file until it agrees".
    pub fn set_tab_width(&mut self, width: usize) {
        self.tab_width = width.max(1);
    }

    /// Which whitespace gets a mark — `whitespace` in `config.toml`.
    pub fn set_whitespace(&mut self, whitespace: Whitespace) {
        self.whitespace = whitespace;
    }

    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    /// The primary head — where the terminal cursor is drawn.
    pub fn cursor(&self) -> Position {
        self.selections.primary().head
    }

    /// Set selections directly. Test-only: production code goes through
    /// actions, so every path a user can take is one a test can take.
    #[doc(hidden)]
    pub fn set_selections_for_test(&mut self, list: Vec<Selection>) {
        assert!(!list.is_empty(), "selections are never empty");
        let mut selections = Selections::single(list[0]);
        for selection in &list[1..] {
            selections.push(*selection);
        }
        self.selections = selections;
    }

    pub fn top_line(&self) -> usize {
        self.top_line
    }

    pub fn left_col(&self) -> usize {
        self.left_col
    }

    pub fn save(&mut self) -> Result<()> {
        self.buffer.save()
    }

    /// Whether the file on disk is byte-for-byte what this buffer holds.
    ///
    /// This is what makes our own save not come back as an external change: the
    /// watcher reports the write we just made, and the answer here is yes, so
    /// nothing happens. No mtime bookkeeping, and no window in which a
    /// remembered timestamp is stale.
    ///
    /// A file that cannot be read at all counts as differing — it has usually
    /// just been deleted, which the caller needs to hear about.
    pub fn matches_disk(&self) -> bool {
        let Some(path) = self.buffer.path() else {
            return false;
        };
        match std::fs::read_to_string(path) {
            // `text_as_saved`, not `text`: the rope holds LF and a CRLF file on
            // disk would never compare equal, so every save of a Windows file
            // would report itself back as an external change.
            Ok(disk) => disk == self.buffer.text_as_saved(),
            Err(_) => false,
        }
    }

    /// Replace the buffer with what is on disk, keeping the cursor where it can
    /// still go.
    ///
    /// Undo history does not survive: it describes edits against a rope that no
    /// longer exists, and offering to undo your way back into a file somebody
    /// else rewrote is worse than starting clean.
    pub fn reload(&mut self) -> Result<()> {
        let Some(path) = self.buffer.path().map(Path::to_path_buf) else {
            return Ok(());
        };
        let selections = self.selections.clone();
        let top_line = self.top_line;

        self.buffer = TextBuffer::from_path(&path)?;
        self.selections = selections;
        self.clamp_selections();
        self.top_line = top_line.min(self.last_line());
        // The tree describes text that is gone. Keeping it would paint the new
        // contents in the old file's colours until the reparse lands, which is
        // worse than painting them plain for one frame — this is the one place
        // a stale highlight is not merely late but wrong.
        self.syntax = None;
        Ok(())
    }

    /// Line contents without the trailing newline.
    pub fn line_text(&self, line: usize) -> String {
        self.buffer.line_text(line)
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    // The app asks through these rather than reaching into `self.buffer`. A
    // panel's internals are not application state — the same rule
    // `RenderContext` enforces pointing the other way.

    pub fn path(&self) -> Option<&Path> {
        self.buffer.path()
    }

    /// Read-only access for the app to take a snapshot to parse.
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// The grammar this buffer's extension claims, if any.
    pub fn language(&self) -> Option<Language> {
        self.language
    }

    /// The newest parse that has landed.
    pub fn syntax(&self) -> Option<&Arc<Syntax>> {
        self.syntax.as_ref()
    }

    /// Apply a completed parse, unless a newer one already landed.
    ///
    /// Two parses cannot be in flight at once by construction — the worker
    /// takes one job at a time — but "cannot" and "cannot, and here is the
    /// counter that proves it" are different claims, and the counter costs a
    /// `u64` and a comparison.
    pub fn set_syntax(&mut self, generation: u64, syntax: Arc<Syntax>) {
        if generation < self.syntax_generation {
            return;
        }
        self.syntax_generation = generation;
        self.syntax = Some(syntax);
    }

    /// The file's name with no dirty marker on it.
    ///
    /// `title()` is what a panel border shows and carries the `*`; the status
    /// bar draws that state as colour instead, so it needs the bare name.
    pub fn file_name(&self) -> String {
        self.buffer
            .path()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string()
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.is_dirty()
    }

    pub fn line_ending(&self) -> LineEnding {
        self.buffer.line_ending()
    }

    /// Collapse to a single caret at `at`, clearing the goal column.
    ///
    /// Every place the old single-cursor code assigned to `self.cursor` now
    /// goes through here, which is what keeps the selection set the only
    /// source of truth. Task 7 replaces these callers with actions.
    pub(crate) fn set_caret(&mut self, at: Position) {
        // Placing the caret ends the undo run, the same as a motion does. This
        // is the mouse's half of that rule: click away mid-word and the next
        // thing typed is a new undo step.
        self.buffer.undo_boundary();
        self.selections.set_single(Selection::caret(at));
        self.goal_col = None;
    }

    /// Cells the gutter occupies for this buffer.
    pub(crate) fn gutter_width(&self) -> usize {
        self.gutter.width(self.buffer.line_count())
    }

    /// A line with a caret on it and nothing selected.
    ///
    /// The only lines that take the current-line tint. A method rather than a
    /// condition written twice, because the gutter and the text both need it
    /// and two copies would drift: a line carrying a selection is deliberately
    /// *not* tinted — the selection already says where the user is — and a
    /// gutter that forgot would light the number on a row whose text stayed
    /// plain.
    pub(crate) fn is_cursor_line(&self, line: usize) -> bool {
        self.selections
            .iter()
            .any(|s| s.is_empty() && s.head.line == line)
    }

    /// Indent-guide levels for a line that is nothing but whitespace.
    ///
    /// The shallower of the two non-blank lines around it, so the guides of the
    /// block above cannot run past its end and into whatever follows. Bounded
    /// in both directions by [`BLANK_GUIDE_SCAN`]; a side that never resumes
    /// inside the bound contributes nothing, and nothing is what gets drawn.
    fn guides_around_blank(&self, line: usize) -> usize {
        let above = (line.saturating_sub(BLANK_GUIDE_SCAN)..line)
            .rev()
            .find_map(|l| {
                self.buffer
                    .with_line_str(l, |text| indent_columns(text, self.tab_width))
            });
        let last = self.buffer.line_count();
        let below = ((line + 1)..(line + 1 + BLANK_GUIDE_SCAN).min(last)).find_map(|l| {
            self.buffer
                .with_line_str(l, |text| indent_columns(text, self.tab_width))
        });
        match (above, below) {
            (Some(above), Some(below)) => above.min(below) / self.tab_width,
            _ => 0,
        }
    }

    /// The area inside the frame, before the gutter is taken out of it.
    fn inner_area(area: Rect) -> Rect {
        typ_core::chrome::inner(area)
    }

    /// The text area: inside the border, and to the right of the gutter.
    ///
    /// This is an instance method rather than a free function precisely because
    /// the gutter's width depends on the buffer. Three callers convert between
    /// screen cells and buffer positions — `render`, `handle_mouse` and
    /// `cursor_position` — and every one of them must subtract the same number.
    /// Routing all three through here is what stops a click landing
    /// `gutter_width` graphemes to the left of the pointer, which is a failure
    /// no test of the gutter's own output would catch.
    fn text_area(&self, area: Rect) -> Rect {
        let inner = Self::inner_area(area);
        let gutter = (self.gutter_width() as u16).min(inner.width);
        Rect {
            x: inner.x + gutter,
            width: inner.width - gutter,
            ..inner
        }
    }

    /// The gutter's own area, to the left of the text.
    fn gutter_area(&self, area: Rect) -> Rect {
        let inner = Self::inner_area(area);
        Rect {
            width: (self.gutter_width() as u16).min(inner.width),
            ..inner
        }
    }

    pub(crate) fn line_grapheme_count(&self, line: usize) -> usize {
        self.buffer.line_grapheme_count(line)
    }

    pub(crate) fn last_line(&self) -> usize {
        self.buffer.line_count().saturating_sub(1)
    }

    /// Keep the cursor inside the viewport after any movement.
    pub(crate) fn scroll_to_cursor(&mut self) {
        let cursor = self.cursor();

        if self.height > 0 {
            if cursor.line < self.top_line {
                self.top_line = cursor.line;
            } else if cursor.line >= self.top_line + self.height {
                self.top_line = cursor.line - self.height + 1;
            }
        }

        if self.width > 0 {
            let col = self.cursor_display_col(cursor);
            if col < self.left_col {
                self.left_col = col;
            } else if col >= self.left_col + self.width {
                // Keep the cursor one column inside the right edge so the
                // character being typed is visible rather than flush against
                // the border.
                self.left_col = col + 1 - self.width;
            }
        }
    }

    /// The display column a cursor sits at, tabs expanded.
    fn cursor_display_col(&self, cursor: Position) -> usize {
        self.buffer.with_line_str(cursor.line, |line| {
            grapheme_to_display_col(line, cursor.col, self.tab_width)
        })
    }

    /// Rows a page motion covers. Before the first frame the height is unknown,
    /// so fall back to a screenful rather than moving nowhere.
    pub(crate) fn page(&self) -> usize {
        self.height.max(1)
    }

    /// Every match in the buffer.
    ///
    /// The app asks through here rather than reaching into `self.buffer`: a
    /// panel's internals are not application state, which is the same rule
    /// `RenderContext` enforces pointing the other way.
    ///
    /// ponytail: this scans the whole buffer — 5.4–8.7 ms on a 50k-line file,
    /// re-measured at v0.2.3 against a 16 ms keystroke budget. Fine for
    /// answering Enter, too slow to run on every keystroke, and the one budget
    /// in the project with less than an order of magnitude of headroom
    /// (gap-analysis defect 38).
    ///
    /// `Ctrl+D` already avoids it: `TextBuffer::find_next` searches from the
    /// cursor and stops at the first hit, at 3.89 µs per press. An incremental
    /// search box wants the same shape — viewport first, the rest completed off
    /// the render thread. See `typ-buffer/tests/perf.rs`.
    pub fn buffer_find_all(&self, query: &SearchQuery) -> Vec<Selection> {
        self.buffer.find_all(query)
    }

    /// Select a range and scroll it into view.
    pub fn select_range(&mut self, selection: Selection) {
        self.selections.set_single(selection);
        self.goal_col = None;
        self.scroll_to_cursor();
    }

    /// Put the caret at the start of a line and centre it in the viewport.
    ///
    /// Centred rather than merely scrolled into view: `scroll_to_cursor` moves
    /// the minimum, which after a jump leaves the target line on whichever edge
    /// it entered from. That is technically visible and useless — you jumped
    /// there to read *around* it, and half the context is off-screen.
    ///
    /// Out-of-range clamps to the last line. Someone typing 9999 means the end
    /// of the file, and erroring at them is pedantry rather than correctness.
    pub fn goto_line(&mut self, line: usize) {
        let line = line.min(self.last_line());
        self.set_caret(Position { line, col: 0 });

        if self.height > 0 {
            // Saturating: near the top of the file there is nothing above to
            // scroll into, and the first screenful is its own context.
            self.top_line = line.saturating_sub(self.height / 2);
        }
        self.scroll_to_cursor();
    }

    /// Replace every match, as one undo step. Returns how many.
    pub fn replace_all(&mut self, query: &SearchQuery, replacement: &str) -> usize {
        let hits = self.buffer.find_all(query);
        if hits.is_empty() {
            return 0;
        }

        // `Other`, so a replace-all is always its own undo step and never folds
        // into a run of typing that happened either side of it.
        self.buffer
            .begin_edit_group(EditKind::Other, &self.selections);
        // Backwards, so each replacement leaves the earlier hits' positions
        // untouched — the same reason multi-caret edits run in reverse.
        for hit in hits.iter().rev() {
            let (start, end) = hit.range();
            self.buffer.replace_range(start, end, replacement);
        }
        self.buffer.end_edit_group();

        self.clamp_selections();
        hits.len()
    }

    /// Pull every selection back inside the text.
    ///
    /// Only replace-all needs this. Undo and redo restore selections that were
    /// recorded against the very rope being restored, so they are in range by
    /// construction; a replace rewrites text underneath selections that were
    /// never recorded anywhere.
    fn clamp_selections(&mut self) {
        let last_line = self.last_line();
        let buffer = &self.buffer;
        let clamp = |p: Position| {
            let line = p.line.min(last_line);
            Position {
                line,
                col: p.col.min(buffer.line_grapheme_count(line)),
            }
        };
        let clamped: Vec<Selection> = self
            .selections
            .iter()
            .map(|s| Selection {
                anchor: clamp(s.anchor),
                head: clamp(s.head),
            })
            .collect();
        self.set_selections(clamped);
        self.goal_col = None;
    }
}

impl Panel for EditorPanel {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn title(&self) -> String {
        let name = self
            .buffer
            .path()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        if self.buffer.is_dirty() {
            format!("{name} *")
        } else {
            name
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        typ_core::chrome::frame(area, buf, &self.title(), ctx, ctx.theme.bg);

        let text_area = self.text_area(area);
        let gutter_area = self.gutter_area(area);

        // Height and width are learned here, and the width is the *text* width:
        // horizontal scrolling measures against the columns text can occupy,
        // not against the ones the gutter has already taken.
        self.height = text_area.height as usize;
        self.width = text_area.width as usize;

        let line_count = self.buffer.line_count();
        let end = (self.top_line + self.height).min(line_count);
        let selections: Vec<Selection> = self.selections.iter().copied().collect();
        let left_col = self.left_col;
        let cursor_line = self.cursor().line;

        // The gutter is furniture, not text: it is drawn into its own area and
        // never windowed by `left_col`, so scrolling a long line sideways moves
        // the code and leaves the numbers standing.
        let gutter_lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                let line =
                    Line::from(
                        self.gutter
                            .render_line(i, cursor_line, line_count, ctx.theme),
                    );
                // The same predicate the text uses, so the tint cannot cover one
                // and miss the other. The spans carry a foreground only, so a
                // background set here survives underneath them.
                if self.is_cursor_line(i) {
                    line.style(Style::default().bg(ctx.theme.cursor_line_bg))
                } else {
                    line
                }
            })
            .collect();
        Paragraph::new(gutter_lines)
            .style(
                Style::default()
                    .fg(ctx.theme.gutter_fg)
                    .bg(ctx.theme.gutter_bg),
            )
            .render(gutter_area, buf);

        // Once per frame, not once per line: the match depends on the cursor,
        // and the search is bounded by the viewport plus a margin so a bracket
        // whose partner is off-screen costs a bounded walk rather than a scan of
        // the file.
        let primary = self.selections.primary();
        let brackets = typ_buffer::brackets::match_at(
            &self.buffer,
            primary.head,
            self.height + BRACKET_SEARCH_MARGIN,
        );
        let text_width = text_area.width as usize;

        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                // Only carets tint their line; a line carrying a real selection
                // is already saying where the user is.
                let cursor_line = self.is_cursor_line(i);
                self.buffer.with_line_str(i, |text| {
                    // Read off the text this row was going to draw anyway, so a
                    // line that says its own depth costs no extra look at the
                    // buffer. Only a blank one pays for the neighbour scan.
                    let indent_guides = match indent_columns(text, self.tab_width) {
                        Some(columns) => columns / self.tab_width,
                        None => self.guides_around_blank(i),
                    };
                    let style = crate::render::LineStyle {
                        line: i,
                        left_col,
                        width: text_width,
                        tab_width: self.tab_width,
                        selections: &selections,
                        primary,
                        cursor_line,
                        brackets,
                        whitespace: self.whitespace,
                        indent_guides,
                        theme: ctx.theme,
                    };
                    crate::render::styled_line(text, &style)
                })
            })
            .collect();
        Paragraph::new(lines)
            .style(Style::default().fg(ctx.theme.fg).bg(ctx.theme.bg))
            .render(text_area, buf);
    }

    fn apply_action(&mut self, action: typ_core::Action) -> Option<Vec<PanelEvent>> {
        self.perform(action)
    }

    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        let inner = self.text_area(panel_area);
        let cursor = self.cursor();
        let row = cursor.line.checked_sub(self.top_line)?;
        if row >= inner.height as usize {
            return None;
        }
        // Scrolled off the left edge is as invisible as scrolled off the right,
        // so both answer None rather than clamping to an edge the cursor is not
        // actually at.
        let col = self.cursor_display_col(cursor).checked_sub(self.left_col)?;
        if col >= inner.width as usize {
            return None;
        }
        Some((inner.x + col as u16, inner.y + row as u16))
    }

    /// The editor has no raw-key behavior left.
    ///
    /// Every key that does anything here is a keymap row resolving to an
    /// `Action`, which is the invariant the whole milestone exists to establish:
    /// a primitive reachable only from a key handler is invisible to the
    /// command palette and to the vim layer. The M1-era arms that used to live
    /// here were the last thing violating it.
    fn handle_key(&mut self, _chord: KeyChord) -> Vec<PanelEvent> {
        Vec::new()
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        let at = |panel: &Self, event: &MouseEvent| {
            // The text area, gutter already subtracted — so a click in the
            // gutter saturates to column 0 and selects the line its number
            // labels, which is what clicking a line number means everywhere.
            let inner = panel.text_area(panel_area);
            let row = event.row.saturating_sub(inner.y) as usize;
            // Both offsets apply: a click is at a screen cell, and the text
            // under it is `top_line` rows down and `left_col` columns across.
            let col = event.column.saturating_sub(inner.x) as usize + panel.left_col;
            let line = (panel.top_line + row).min(panel.last_line());
            Position {
                line,
                col: panel.buffer.with_line_str(line, |text| {
                    display_to_grapheme_col(text, col, panel.tab_width)
                }),
            }
        };

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let position = at(self, &event);

                if event.modifiers.contains(KeyModifiers::ALT) {
                    // Alt+click stacks a cursor: the mouse half of
                    // multi-cursor, with Action::AddCursor as the keyboard half.
                    self.selections.push(Selection::caret(position));
                    self.last_click = Some(position);
                    self.drag_anchor = Some(position);
                    return vec![PanelEvent::NeedsRedraw];
                }

                if self.last_click == Some(position) {
                    // A second click in the same cell selects the word under
                    // it. No timing check: clicking the same cell twice is
                    // deliberate, and a double-click timer would put a clock on
                    // the render path to distinguish two things a user does not
                    // confuse.
                    let text = self.buffer.line_text(position.line);
                    if let Some((start, end)) = typ_buffer::word_at(&text, position.col) {
                        self.selections.set_single(Selection {
                            anchor: Position {
                                line: position.line,
                                col: start,
                            },
                            head: Position {
                                line: position.line,
                                col: end,
                            },
                        });
                        self.drag_anchor = None;
                        self.goal_col = None;
                        return vec![PanelEvent::NeedsRedraw];
                    }
                }

                self.set_caret(position);
                self.drag_anchor = Some(position);
                self.last_click = Some(position);
                vec![PanelEvent::NeedsRedraw]
            }

            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(anchor) = self.drag_anchor else {
                    // A drag with no press behind it is not ours: it belongs to
                    // whatever panel the press landed in.
                    return Vec::new();
                };
                let head = at(self, &event);
                self.selections.set_single(Selection { anchor, head });
                self.goal_col = None;
                vec![PanelEvent::NeedsRedraw]
            }

            MouseEventKind::Up(MouseButton::Left) => {
                self.drag_anchor = None;
                Vec::new()
            }

            // Invariant 8 — mouse and keyboard are peers. A clipboard reachable
            // only from the keyboard is half a feature.
            //
            // Right-click *inside* a selection copies it and leaves it standing.
            // Outside one it does nothing: the alternative is copying whatever
            // happens to be selected elsewhere, which silently replaces the
            // clipboard on a misclick.
            MouseEventKind::Down(MouseButton::Right) => {
                let position = at(self, &event);
                let inside = self
                    .selections
                    .iter()
                    .any(|s| !s.is_empty() && s.range().0 <= position && position < s.range().1);
                if !inside {
                    return Vec::new();
                }
                self.perform(typ_core::Action::Copy).unwrap_or_default()
            }

            // Middle-click pastes at the pointer, the X11 convention every
            // terminal user already has in their hands.
            MouseEventKind::Down(MouseButton::Middle) => {
                let position = at(self, &event);
                self.set_caret(position);
                self.last_click = Some(position);
                self.perform(typ_core::Action::Paste).unwrap_or_default()
            }

            _ => Vec::new(),
        }
    }

    fn handle_scroll(&mut self, delta: i32, _panel_area: Rect) -> Vec<PanelEvent> {
        let max_top = self.buffer.line_count().saturating_sub(self.height.max(1));
        self.top_line = (self.top_line as i64 + delta as i64).clamp(0, max_top as i64) as usize;
        vec![PanelEvent::NeedsRedraw]
    }

    fn needs_close_confirmation(&self) -> Option<String> {
        self.buffer
            .is_dirty()
            .then(|| "Unsaved changes. Close anyway?".to_string())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Display columns of leading whitespace, or `None` for a line that is nothing
/// but whitespace.
///
/// `None` rather than zero, because a blank line is not evidence of depth zero
/// — it is evidence of nothing, and the caller looks at its neighbours instead.
/// Bytes rather than graphemes: only ASCII space and tab are indentation, and
/// neither can be part of a multi-byte sequence.
fn indent_columns(text: &str, tab_width: usize) -> Option<usize> {
    let mut column = 0usize;
    for byte in text.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += tab_width - (column % tab_width),
            // `with_line_str` hands back the terminator too, so the end of the
            // line arrives here rather than as the end of the iterator.
            b'\n' | b'\r' => return None,
            _ => return Some(column),
        }
    }
    None
}
