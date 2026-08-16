use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

use crate::line_ending::LineEnding;
use crate::position::Position;
use crate::search::SearchQuery;
use crate::selection::{Selection, Selections};
use crate::undo::{EditKind, History};

pub struct TextBuffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
    history: History,
    /// Nesting depth of `begin_edit_group`. While non-zero, individual edits
    /// stop taking their own snapshots, so a multi-caret edit is one undo step
    /// rather than one per cursor.
    group_depth: usize,
    /// Detected once at load. Recorded rather than recomputed because editing
    /// the file must not change the answer — a user deleting the first line
    /// does not thereby convert the file to LF.
    line_ending: LineEnding,
}

impl TextBuffer {
    // Named to match `Rope::from_str`, not the `FromStr` trait: construction is
    // infallible, so a `Result`-returning trait impl would be the wrong shape.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
            path: None,
            dirty: false,
            history: History::default(),
            group_depth: 0,
            line_ending: LineEnding::detect(s),
        }
    }

    /// An empty buffer that will be written to `path` when saved.
    ///
    /// A sibling of `from_path` rather than a flag on it, so "read this file"
    /// keeps meaning exactly that and never quietly invents one.
    ///
    /// Not dirty: nothing has been typed. Marking it dirty would make Ctrl+Q
    /// challenge the user over a file they never edited.
    pub fn new_at(path: &Path) -> Self {
        Self {
            rope: Rope::new(),
            path: Some(path.to_path_buf()),
            dirty: false,
            history: History::default(),
            group_depth: 0,
            // Nothing to detect from, and a file TYPE is about to create has no
            // existing convention to honour.
            line_ending: LineEnding::default(),
        }
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Ok(Self {
            line_ending: LineEnding::detect(&text),
            rope: Rope::from_str(&text),
            path: Some(path.to_path_buf()),
            dirty: false,
            history: History::default(),
            group_depth: 0,
        })
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// The line terminator this file was loaded with.
    ///
    /// Detection only: `save` still writes LF regardless. Preserving it is
    /// M2.5's job, and this is the half the status bar needs to stop claiming
    /// every file is LF.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Call `f` with one line's text, borrowed from the rope when possible.
    ///
    /// `RopeSlice::as_str` succeeds whenever the line lives inside a single
    /// chunk, which is the overwhelmingly common case — ropey chunks are ~1 KB
    /// and lines of code are not. Only a line straddling a chunk boundary pays
    /// for a `String`.
    ///
    /// This exists because `line_text` returning an owned `String` was correct
    /// but quadratic in aggregate: three callers looped it over every line in
    /// the buffer, so one keystroke on a 50k-line file allocated 50k strings.
    /// A borrowing accessor makes the cheap thing the easy thing to reach for.
    pub fn with_line_str<T>(&self, line: usize, f: impl FnOnce(&str) -> T) -> T {
        if line >= self.rope.len_lines() {
            return f("");
        }
        with_slice_str(self.rope.line(line), f)
    }

    /// Line contents without the trailing newline.
    ///
    /// Allocates. Prefer `with_line_str` in anything that runs per line over a
    /// range of lines.
    pub fn line_text(&self, line: usize) -> String {
        self.with_line_str(line, str::to_string)
    }

    /// Graphemes on a line, without materializing it.
    pub fn line_grapheme_count(&self, line: usize) -> usize {
        self.with_line_str(line, |s| s.graphemes(true).count())
    }

    /// Absolute char offset of a `Position`, clamping out-of-range input.
    fn char_offset(&self, pos: Position) -> usize {
        let line = pos.line.min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let chars_before: usize = self.with_line_str(line, |text| {
            text.graphemes(true)
                .take(pos.col)
                .map(|g| g.chars().count())
                .sum()
        });
        line_start + chars_before
    }

    pub fn insert_char(&mut self, pos: Position, ch: char) {
        self.record_snapshot(pos);
        let offset = self.char_offset(pos);
        self.rope.insert_char(offset, ch);
        self.dirty = true;
    }

    /// Delete the grapheme immediately before `pos` (backspace).
    pub fn delete_before(&mut self, pos: Position) {
        let offset = self.char_offset(pos);
        if offset == 0 {
            return;
        }
        let n = if pos.col == 0 {
            1 // joining with the previous line: remove the newline
        } else {
            self.with_line_str(pos.line, |text| {
                text.graphemes(true)
                    .nth(pos.col - 1)
                    .map_or(1, |g| g.chars().count())
            })
        };
        self.record_snapshot(pos);
        self.rope.remove(offset - n..offset);
        self.dirty = true;
    }

    /// Delete the grapheme at `pos` (forward delete).
    ///
    /// At the end of a line this removes the newline, joining the next line up.
    pub fn delete_after(&mut self, pos: Position) {
        let offset = self.char_offset(pos);
        if offset >= self.rope.len_chars() {
            return;
        }
        let n = self.with_line_str(pos.line, |text| {
            text.graphemes(true)
                .nth(pos.col)
                .map_or(1, |g| g.chars().count())
        });
        self.record_snapshot(pos);
        self.rope.remove(offset..offset + n);
        self.dirty = true;
    }

    /// The text between two positions.
    ///
    /// Ordered by the caller — a selection's `range()` already answers which end
    /// comes first, so this does not second-guess it.
    pub fn text_in_range(&self, start: Position, end: Position) -> String {
        let from = self.char_offset(start);
        let to = self.char_offset(end);
        if from >= to {
            return String::new();
        }
        self.rope.slice(from..to).to_string()
    }

    /// Every match in the buffer, in document order, as selections whose head
    /// sits at the end of the match — so jumping to one leaves the cursor
    /// where typing would naturally continue.
    pub fn find_all(&self, query: &SearchQuery) -> Vec<Selection> {
        // Split once for the whole buffer, not once per line.
        let needle: Vec<&str> = query.needle.graphemes(true).collect();

        let mut hits = Vec::new();
        // `rope.lines()` walks the tree once. Indexing `rope.line(i)` in a loop
        // instead is a fresh O(log n) descent per line, which measured at 458 ns
        // of pure overhead per line — 23 ms across 50k lines before a single
        // byte of the search ran.
        for (line, slice) in self.rope.lines().enumerate() {
            with_slice_str(slice, |text| {
                for (start, end) in crate::search::find_in_line_with(text, &needle, query) {
                    hits.push(Selection {
                        anchor: Position { line, col: start },
                        head: Position { line, col: end },
                    });
                }
            });
        }
        hits
    }

    /// Replace the text between two positions as a single undo step.
    ///
    /// An empty range inserts, so callers can express insertion, deletion and
    /// replacement as one operation and not branch three ways.
    pub fn replace_range(&mut self, start: Position, end: Position, text: &str) {
        let from = self.char_offset(start);
        let to = self.char_offset(end);
        if from > to || (from == to && text.is_empty()) {
            return;
        }
        self.record_snapshot(start);
        if to > from {
            self.rope.remove(from..to);
        }
        if !text.is_empty() {
            self.rope.insert(from, text);
        }
        self.dirty = true;
    }

    /// Take an undo snapshot unless an edit group is open.
    ///
    /// Only the M1-era standalone helpers reach this. They have no selection set
    /// and no edit kind to offer, so they record as `Other` at a caret placed
    /// where they are editing — which reproduces their old one-step-per-call
    /// behavior exactly. M2 Task 12 deletes their last callers.
    fn record_snapshot(&mut self, at: Position) {
        if self.group_depth == 0 {
            let selections = Selections::single(Selection::caret(at));
            self.history
                .record(EditKind::Other, self.rope.clone(), &selections);
        }
    }

    /// Begin a group of edits that undo together.
    ///
    /// One snapshot is taken up front and none during the group, so thirty
    /// cursors typing one character is one undo step. Without this, undoing a
    /// thirty-caret edit would take thirty presses and leave the buffer in
    /// states the user never typed.
    ///
    /// Whether that snapshot is actually pushed is `History`'s call: a group
    /// continuing a run of the same kind folds into the one already there.
    pub fn begin_edit_group(&mut self, kind: EditKind, selections: &Selections) {
        if self.group_depth == 0 {
            self.history.record(kind, self.rope.clone(), selections);
        }
        self.group_depth += 1;
    }

    pub fn end_edit_group(&mut self) {
        self.group_depth = self.group_depth.saturating_sub(1);
    }

    /// How many undo steps are currently held.
    pub fn undo_depth(&self) -> usize {
        self.history.depth()
    }

    /// End the current undo run. The next edit starts a new step.
    pub fn undo_boundary(&mut self) {
        self.history.boundary();
    }

    /// Undo one step, returning the selections to restore.
    ///
    /// `None` means there was nothing to undo, so the caller leaves its
    /// selections alone.
    pub fn undo(&mut self, current: &Selections) -> Option<Selections> {
        let snapshot = self.history.undo(self.rope.clone(), current)?;
        self.rope = snapshot.rope;
        self.dirty = true;
        Some(snapshot.selections)
    }

    pub fn redo(&mut self, current: &Selections) -> Option<Selections> {
        let snapshot = self.history.redo(self.rope.clone(), current)?;
        self.rope = snapshot.rope;
        self.dirty = true;
        Some(snapshot.selections)
    }

    /// Write the buffer to disk, atomically.
    ///
    /// The content goes to a sibling temporary file, is flushed to the device,
    /// and is then renamed over the target. `rename` replaces the destination
    /// in one step on both NTFS and POSIX, so an interrupted save leaves the
    /// previous file intact rather than a truncated one. Writing in place would
    /// mean a crash between truncate and write costs the user the whole file
    /// rather than the last edit.
    pub fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .context("buffer has no path to save to")?
            .clone();

        // Same directory, so the rename never crosses a filesystem boundary —
        // across devices it would silently become a copy, which is not atomic.
        let temp = temp_path_beside(&path);
        write_all_and_sync(&temp, &self.rope)
            .with_context(|| format!("writing {}", temp.display()))?;

        if let Err(e) = std::fs::rename(&temp, &path) {
            // Leave nothing behind on failure; the original is untouched.
            let _ = std::fs::remove_file(&temp);
            return Err(e).with_context(|| format!("replacing {}", path.display()));
        }

        self.dirty = false;
        Ok(())
    }

    /// Point the buffer at another path. Test-only: production code opens a
    /// new buffer rather than redirecting one.
    #[doc(hidden)]
    pub fn set_path_for_test(&mut self, path: PathBuf) {
        self.path = Some(path);
    }
}

/// Call `f` with a line slice's text, borrowed from the rope when possible.
///
/// Free-standing rather than a method so callers holding a slice from
/// `Rope::lines()` can use it without paying for a second lookup by index.
fn with_slice_str<T>(slice: ropey::RopeSlice, f: impl FnOnce(&str) -> T) -> T {
    match slice.as_str() {
        Some(s) => f(trim_line_ending(s)),
        None => {
            let owned = slice.to_string();
            f(trim_line_ending(&owned))
        }
    }
}

/// A line without its terminator. Handles CRLF as one unit rather than as two
/// separate trims, so a stray `\r` inside a line is left alone.
fn trim_line_ending(s: &str) -> &str {
    s.strip_suffix('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s))
        .unwrap_or(s)
}

/// A sibling of `path` that will not collide with a real file, or with another
/// instance of TYPE saving the same file.
///
/// The pid is what makes the second guarantee. Two editors saving one path with
/// a fixed temp name race: one truncates the other's half-written file and
/// renames whichever won, and the loser's content is gone. A kill mid-save also
/// leaves the file behind, and a pid-suffixed one is at least attributable.
fn temp_path_beside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "buffer".to_string());
    let parent = path.parent().unwrap_or(Path::new("."));
    parent.join(format!(".{name}.{}.typ-tmp", std::process::id()))
}

/// Write the rope out and flush it to the device before returning.
///
/// Without the flush, the rename can be durable while the contents are not —
/// which produces an empty file after a power loss, the exact failure the
/// atomic write exists to prevent.
fn write_all_and_sync(path: &Path, rope: &Rope) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    for chunk in rope.chunks() {
        file.write_all(chunk.as_bytes())?;
    }
    file.flush()?;
    file.sync_all()?;
    Ok(())
}
