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
    /// Bumped on every change to the text, and never reset.
    ///
    /// `dirty` cannot answer "did the text change?" — it is false again after
    /// a save and says nothing about an edit that returned the buffer to what
    /// was on disk. A monotonic counter is what lets the app ask for a reparse
    /// exactly when there is something new to parse, rather than hooking every
    /// call site that might have edited something and missing one.
    revision: u64,
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
            revision: 0,
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
            revision: 0,
        }
    }

    /// Read a file into a buffer.
    ///
    /// **CRLF is normalized to LF in the rope** and the original recorded in
    /// `line_ending`, which `save` writes back. Keeping the `\r` in the rope
    /// would put it inside every line as a grapheme that `col` arithmetic, word
    /// motion and search all have to know to skip — and an editor whose whole
    /// cursor model is "col is a grapheme index" cannot afford one grapheme
    /// that is secretly punctuation. TermIDE takes the same approach for the
    /// same reason.
    pub fn from_path(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let line_ending = LineEnding::detect(&text);
        let text = match line_ending {
            LineEnding::Lf => text,
            LineEnding::Crlf => text.replace("\r\n", "\n"),
        };
        Ok(Self {
            line_ending,
            revision: 0,
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

    /// Bytes of text, without allocating any of them.
    ///
    /// `text().len()` answers the same question by copying the whole file to
    /// do it, which is the trap AGENTS.md names about `line_text`.
    pub fn byte_len(&self) -> usize {
        self.rope.len_bytes()
    }

    /// The whole buffer as a `String`.
    ///
    /// Allocates the entire text, so it is for whole-file work and never for
    /// anything on the keystroke path.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// A consistent copy of the text, for a worker to read while editing
    /// continues.
    ///
    /// Ropey's nodes are reference-counted and shared, so this is cheap and
    /// the clone diverges from the original only where one of them is edited.
    /// Zed builds the same property deliberately into its sum trees; here it
    /// comes with the buffer that was already chosen.
    ///
    /// Note what this is *not*: `text()` allocates the whole file and is the
    /// trap AGENTS.md names about `line_text`. A snapshot is the cheap one and
    /// is what anything off-thread should take.
    pub fn snapshot(&self) -> Rope {
        self.rope.clone()
    }

    /// The whole buffer as `save` would write it, line endings and all.
    ///
    /// The rope holds LF only. Comparing `text()` against a CRLF file on disk
    /// says they differ when they do not, which would make every save of a
    /// Windows file report itself as an external change.
    pub fn text_as_saved(&self) -> String {
        match self.line_ending {
            LineEnding::Lf => self.text(),
            LineEnding::Crlf => self.text().replace('\n', "\r\n"),
        }
    }

    /// The line terminator this file was loaded with, and the one `save`
    /// writes back. The rope itself holds LF only.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// How many times the text has changed, ever.
    ///
    /// Compare two readings to know whether anything needs reparsing. Never
    /// compare it across buffers: a freshly opened file starts at zero again,
    /// so the caller invalidates rather than compares when the buffer itself
    /// is replaced.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The text changed.
    ///
    /// Every mutation goes through here so `dirty` and `revision` cannot drift
    /// apart — the alternative was six call sites each remembering to set two
    /// fields, which is five chances to set one.
    fn touch(&mut self) {
        self.dirty = true;
        self.revision += 1;
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

    /// The first `limit` lines, borrowed, without their terminators.
    ///
    /// For scanning, not for reading content: a line long enough to straddle a
    /// ropey chunk — roughly a kilobyte — comes back empty rather than being
    /// copied, because the point of this accessor is that it never allocates.
    /// A scan that would be wrong about such a line wants `with_line_str`.
    pub fn lines_str(&self, limit: usize) -> impl Iterator<Item = &str> {
        self.rope
            .lines()
            .take(limit)
            .map(|slice| slice.as_str().map_or("", trim_line_ending))
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

    /// The rope behind this buffer.
    ///
    /// Exposed for `typ-lsp`, which counts UTF-8, UTF-16 and UTF-32 offsets
    /// against a line and must not copy one out to do it — that is the
    /// `line_text` trap at the scale of a whole file. Read-only by
    /// construction: every mutation still goes through this type.
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Absolute char offset of a `Position`, clamping out-of-range input.
    ///
    /// **The grapheme boundary.** `col` is a grapheme index and everything
    /// below TYPE — ropey, tree-sitter, LSP — counts something else. This and
    /// [`position`](Self::position) are the only two places the two units meet.
    pub fn char_index(&self, pos: Position) -> usize {
        self.char_offset(pos)
    }

    /// The `Position` a char offset falls in, snapping into a grapheme.
    ///
    /// **Snaps down**, to the start of the cluster the offset is inside. A
    /// language server may legitimately answer with an offset between the two
    /// code points of an emoji; there is no `Position` for that and
    /// `Selections` could not hold one. Snapping up would move a cursor past
    /// the text the server was pointing at.
    pub fn position(&self, char_idx: usize) -> Position {
        let char_idx = char_idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(char_idx);
        let into_line = char_idx - self.rope.line_to_char(line);

        let col = self.with_line_str(line, |text| {
            let mut start = 0usize;
            let mut count = 0usize;
            for (col, grapheme) in text.graphemes(true).enumerate() {
                let end = start + grapheme.chars().count();
                // Strictly inside this cluster, which includes starting it.
                // Testing the *start* instead skips a cluster the offset lands
                // in the middle of, and reports the one after it.
                if into_line < end {
                    return col;
                }
                start = end;
                count = col + 1;
            }
            // At or past the end of the line's content, so the offset is in
            // the line terminator and the answer is the end of the line.
            count
        });

        Position { line, col }
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
        self.touch();
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
        self.touch();
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
        self.touch();
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

    /// The first match strictly after `after`, wrapping to the top of the
    /// buffer if there is none below it.
    ///
    /// This exists so `Ctrl+D` is not `find_all` with a filter on it. `find_all`
    /// scans the whole buffer — measured at ~7 ms on 50k lines, against a 16 ms
    /// keystroke budget — and select-next-occurrence is a key people *hold*, so
    /// one scan per press is not a cost that can be paid. Stopping at the first
    /// hit is both the faster thing and the simpler one.
    ///
    /// Wrapping is unconditional, and it is load-bearing rather than a
    /// convenience: coming back round to a match the caller already holds is how
    /// `Ctrl+D` knows every occurrence is selected and it is time to stop.
    pub fn find_next(&self, query: &SearchQuery, after: Position) -> Option<Selection> {
        if query.needle.is_empty() {
            return None;
        }
        let needle: Vec<&str> = query.needle.graphemes(true).collect();

        let line_count = self.rope.len_lines();
        let first_on_line = |line: usize, min_col: Option<usize>| -> Option<Selection> {
            self.with_line_str(line, |text| {
                crate::search::find_in_line_with(text, &needle, query)
                    .into_iter()
                    .find(|(start, _)| min_col.is_none_or(|min| *start > min))
                    .map(|(start, end)| Selection {
                        anchor: Position { line, col: start },
                        head: Position { line, col: end },
                    })
            })
        };

        // Forward from the cursor's line to the end...
        for line in after.line..line_count {
            let min_col = (line == after.line).then_some(after.col);
            if let Some(hit) = first_on_line(line, min_col) {
                return Some(hit);
            }
        }
        // ...then round to the top and back up to it, inclusive, so a lone match
        // behind the cursor is still found.
        for line in 0..=after.line.min(line_count.saturating_sub(1)) {
            if let Some(hit) = first_on_line(line, None) {
                return Some(hit);
            }
        }
        None
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
        self.touch();
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
        self.touch();
        Some(snapshot.selections)
    }

    pub fn redo(&mut self, current: &Selections) -> Option<Selections> {
        let snapshot = self.history.redo(self.rope.clone(), current)?;
        self.rope = snapshot.rope;
        self.touch();
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

        // Write through a symlink rather than over it. The rename replaces
        // whatever is at the path, so saving `~/.bashrc` when it is a link into
        // a dotfiles repo would replace the link with a regular file and
        // silently detach it from the repo. ttt resolves the link for the same
        // reason; nothing else in the surveyed field does.
        let target = resolve_symlink(&path);

        // Same directory, so the rename never crosses a filesystem boundary —
        // across devices it would silently become a copy, which is not atomic.
        let temp = temp_path_beside(&target);
        write_all_and_sync(&temp, &self.rope, self.line_ending)
            .with_context(|| format!("writing {}", temp.display()))?;

        // Carry the original's mode onto the temp file *before* the rename, so
        // the file is never briefly world-readable and an executable script
        // does not stop being executable because somebody edited it.
        if let Err(e) = copy_permissions(&target, &temp) {
            let _ = std::fs::remove_file(&temp);
            return Err(e).with_context(|| format!("preserving the mode of {}", target.display()));
        }

        if let Err(e) = std::fs::rename(&temp, &target) {
            // Leave nothing behind on failure; the original is untouched.
            let _ = std::fs::remove_file(&temp);
            return Err(e).with_context(|| format!("replacing {}", target.display()));
        }

        // A rename is not durable until the directory entry naming it is. Skip
        // this and a power loss can leave the directory pointing at neither
        // file — which is the zero-length-file outcome the atomic write exists
        // to prevent, arriving by the other door. None of ttt, TermIDE or Fresh
        // does this.
        sync_parent_dir(&target);

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
fn write_all_and_sync(path: &Path, rope: &Rope, ending: LineEnding) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    for chunk in rope.chunks() {
        match ending {
            // The rope holds LF. A chunk boundary cannot split a `\n`, so
            // converting per chunk is safe without carrying state across them.
            LineEnding::Lf => file.write_all(chunk.as_bytes())?,
            LineEnding::Crlf => file.write_all(chunk.replace('\n', "\r\n").as_bytes())?,
        }
    }
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// The real file behind a path, if the path is a symlink.
///
/// Only follows when the path *is* a link: `canonicalize` on a plain path is a
/// syscall for nothing, and on Windows it returns a `\\?\` form that is worth
/// not introducing where it is not needed.
fn resolve_symlink(path: &Path) -> PathBuf {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        _ => path.to_path_buf(),
    }
}

/// Give `to` the permissions `from` has, when `from` exists.
///
/// A file being created for the first time has nothing to copy, which is not a
/// failure.
fn copy_permissions(from: &Path, to: &Path) -> std::io::Result<()> {
    let Ok(meta) = std::fs::metadata(from) else {
        return Ok(());
    };
    std::fs::set_permissions(to, meta.permissions())
}

/// fsync the directory holding `path`, so the rename that named the file is
/// durable and not only the bytes inside it.
///
/// Best-effort: opening a directory for this is not portable — Windows has no
/// equivalent and returns an error — and a save that worked must not be
/// reported as failed because the extra durability step was unavailable.
fn sync_parent_dir(path: &Path) {
    let Some(parent) = path.parent() else { return };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
}
