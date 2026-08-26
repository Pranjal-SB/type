//! Tabs: the list of open files, and everything that changes which one you are
//! looking at.
//!
//! A child module rather than a sibling, for the reason `search.rs` is one: it
//! reaches `App`'s private fields without any of them widening to `pub(crate)`.
//! The extraction is meant to shorten `app.rs`, not to open it up.
//!
//! Nothing here changed in the move. Tabs were the responsibility that arrived
//! most recently and never got a home of their own — `picker.rs` and
//! `search.rs` were extracted when they grew and M2.9's tabs were not.

use std::path::Path;

use anyhow::Result;
use ratatui::layout::Rect;
use typ_core::Panel;
use typ_panel_editor::EditorPanel;

use crate::app::{App, Focus, Tab};

impl App {
    /// Open a file: switch to it if it is open, else give it a tab.
    ///
    /// **This used to be the one path in the editor that could lose work**, and
    /// it carried a confirmation to say so, because an open replaced the buffer.
    /// A new tab replaces nothing, so the question and the state that
    /// remembered the answer are both gone. The guard moves to *closing* a tab,
    /// where the work is actually at risk.
    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        if let Some(index) = self.tab_for(path) {
            self.activate_tab(index);
            // `activate_tab` returns early when the tab is already active, so
            // this is not redundant: opening the file already on screen, from
            // the tree, still means "put me in the editor".
            self.focus = Focus::Editor;
            return Ok(());
        }

        // `typ` with no arguments starts on an empty untitled buffer. Appending
        // beside it would leave every session with a first tab that can never
        // become useful. Only when nobody has typed in it — an untitled buffer
        // with work in it is exactly what the old open guard protected.
        let scratch = self.tabs.len() == 1
            && self.tabs[0].panel.path().is_none()
            && !self.tabs[0].panel.is_dirty();
        if scratch {
            self.tabs[0] = Tab::new(self.panel_for(path)?);
            self.settle_active_tab();
            return Ok(());
        }

        self.open_in_new_tab(path)
    }

    /// Open every path, and leave the first one on screen.
    ///
    /// **What `typ a.rs b.rs` means.** Before tabs it meant `typ a.rs`, with
    /// everything after the first path dropped silently; the gap analysis
    /// called that "honest until tabs exist, a real bug the moment they do".
    ///
    /// The first stays active because that is what `vim a b` and `code a b`
    /// both do: the rest are context you asked to have open, not the thing you
    /// asked to look at. A path that cannot be opened stops the whole thing
    /// rather than being skipped — `$EDITOR`'s caller needs the non-zero exit,
    /// and quietly opening the other three is how a typo costs an afternoon.
    pub fn open_all(&mut self, paths: &[std::path::PathBuf]) -> Result<()> {
        for path in paths {
            self.open_path(path)?;
        }
        // Not `activate_tab(0)` unconditionally: with one path it is already
        // active, and `activate_tab` would be a no-op that still reads as a
        // decision.
        if paths.len() > 1 {
            self.activate_tab(0);
        }
        Ok(())
    }

    /// The tab already holding `path`, if there is one.
    ///
    /// Compared canonically: a picker produces `src/main.rs`, a tree produces an
    /// absolute path and a command line can produce `./src/main.rs`, and all
    /// three are the same file. `canonicalize` fails on a path with no file
    /// behind it, which is a normal case here — opening a name that does not
    /// exist yet — so it falls back to comparing what it was given.
    fn tab_for(&self, path: &Path) -> Option<usize> {
        fn resolve(path: &Path) -> std::path::PathBuf {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        let wanted = resolve(path);
        self.tabs
            .iter()
            .position(|tab| tab.panel.path().is_some_and(|open| resolve(open) == wanted))
    }

    /// Open `path` alongside whatever is already open, and switch to it.
    ///
    /// No guard, because nothing is being replaced. That is the difference tabs
    /// make and it is why `open_path`'s confirmation goes away with them.
    pub fn open_in_new_tab(&mut self, path: &Path) -> Result<()> {
        let tab = Tab::new(self.panel_for(path)?);
        self.tabs.push(tab);
        // Before `settle_active_tab`, which writes the configured indent width
        // and whitespace into `tabs[active]` — run it while `active` still
        // points at the tab being left and the settings land on the wrong file.
        self.active = self.tabs.len() - 1;
        self.settle_active_tab();
        Ok(())
    }

    /// Make the tab at `index` the visible one.
    pub fn activate_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active {
            return;
        }
        self.active = index;
        self.settle_active_tab();
        self.mark_dirty();
    }

    /// The next open file, wrapping at the end.
    pub fn next_tab(&mut self) {
        let next = (self.active + 1) % self.tabs.len();
        self.activate_tab(next);
    }

    /// The previous open file, wrapping at the start.
    pub fn prev_tab(&mut self) {
        let previous = (self.active + self.tabs.len() - 1) % self.tabs.len();
        self.activate_tab(previous);
    }

    /// Close the tab at `index`, whether or not it is the active one.
    ///
    /// **No guard here.** The unsaved-work question belongs to the key that
    /// asked, because the answer is "press it again" — see `request_close_tab`.
    /// Callers that reach this directly, like a click on a close box, have
    /// already asked or have decided not to.
    pub fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }

        // Never zero tabs: `editor()` would have to return an `Option` and
        // every one of its callers would handle a state with no meaning. The
        // last one closing leaves the empty buffer the editor starts in.
        if self.tabs.len() == 1 {
            self.tabs[0] = Tab::new(EditorPanel::from_str(""));
            self.settle_active_tab();
            self.mark_dirty();
            return;
        }

        self.tabs.remove(index);
        // Closing a tab that is not on screen must not change what is. Its
        // index moves when an earlier one goes, which is the whole reason an
        // index is not a handle.
        if index != self.active {
            if index < self.active {
                self.active -= 1;
            }
            self.mark_dirty();
            return;
        }

        self.active = self.most_recently_used();
        self.settle_active_tab();
        self.mark_dirty();
    }

    /// The tab with the highest `last_used` stamp.
    fn most_recently_used(&self) -> usize {
        self.tabs
            .iter()
            .enumerate()
            .max_by_key(|(_, tab)| tab.last_used)
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    /// Close the tab at `index`, asking first if it holds unsaved work.
    ///
    /// The same shape `request_quit` uses, and for the same reason: the only
    /// thing that answers "you will lose this" is doing it again.
    ///
    /// **Every close gesture goes through here** — Ctrl+W, the close box and a
    /// middle click. Invariant 8 makes the mouse and the keyboard peers, and a
    /// close box that skipped the question would be the one path in the editor
    /// that loses work in a single click without saying anything.
    pub(crate) fn request_close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.close_pending != Some(index)
            && let Some(message) = self.tabs[index].panel.needs_close_confirmation()
        {
            self.status = Some(format!(
                "{message}  Close {} again to discard, Ctrl+S to save.",
                self.tabs[index].panel.file_name()
            ));
            self.close_pending = Some(index);
            return;
        }
        self.close_tab(index);
    }

    /// Build the panel for a path, whether or not there is a file behind it.
    ///
    /// A path with no file is one to create, not an error: `typ notes.md` and
    /// opening a not-yet-existing file from the tree are the same operation, so
    /// they take the same branch.
    fn panel_for(&self, path: &Path) -> Result<EditorPanel> {
        // The registry decides the handler. There is one content panel today,
        // but the lookup runs from day one so adding viewers never touches this.
        let _handler = self.registry.handler_for(path);
        if path.exists() {
            EditorPanel::from_path(path)
        } else {
            Ok(EditorPanel::new_at(path))
        }
    }

    /// Everything that has to happen when a different buffer becomes visible.
    ///
    /// Called from opening *and* from switching, because the two leave the app
    /// in the same place: config the new panel has never seen, a watch pointed
    /// at the file being left, and a buffer that may want parsing.
    fn settle_active_tab(&mut self) {
        // Every path that makes a tab active lands here, which is what keeps
        // the stamp honest — a switch that forgot it would make the tab look
        // older than one nobody has touched.
        self.next_use += 1;
        self.tabs[self.active].last_used = self.next_use;

        self.apply_indent_width();
        self.tabs[self.active].panel.set_whitespace(self.whitespace);
        self.focus = Focus::Editor;
        self.rewatch();
        self.request_parse_if_stale();
    }

    /// Where the tab bar is, or a zero-height rect when there is not one.
    pub fn tab_bar_area(&self, area: Rect) -> Rect {
        let (body, _) = crate::layout::split_frame(area);
        let (_, pane) = crate::layout::split(body);
        crate::layout::split_tabs(pane, self.tabs.len()).0
    }

    /// Give a mouse event to the tab bar. Returns whether the bar took it.
    ///
    /// **The bar is opaque.** A click on its empty right-hand end is consumed
    /// rather than passed down, because what is underneath is the editor's
    /// first line and moving the caret there is not what anyone aiming at a
    /// tab strip meant to do.
    ///
    /// Cells come from `tabbar::cells`, the same call the renderer makes, so
    /// "the tab under the pointer" is true by construction rather than by two
    /// pieces of arithmetic agreeing about the scroll offset.
    pub fn route_tab_bar_mouse(&mut self, event: crossterm::event::MouseEvent, area: Rect) -> bool {
        use crossterm::event::{MouseButton, MouseEventKind};

        let bar = self.tab_bar_area(area);
        if bar.height == 0
            || event.row != bar.y
            || event.column < bar.x
            || event.column >= bar.right()
        {
            return false;
        }

        let labels: Vec<String> = self.tabs.iter().map(|tab| tab.panel.title()).collect();
        let x = event.column - bar.x;
        let Some(cell) = crate::tabbar::cells(&labels, self.active, bar.width)
            .into_iter()
            .find(|cell| x >= cell.x && x < cell.x + cell.width)
        else {
            return true;
        };

        match event.kind {
            // The convention every browser and terminal already carries.
            MouseEventKind::Down(MouseButton::Middle) => self.request_close_tab(cell.index),
            MouseEventKind::Down(MouseButton::Left) => {
                if crate::tabbar::close_box_x(&cell, &labels[cell.index]) == Some(x) {
                    self.request_close_tab(cell.index);
                } else {
                    // Anything that is not a close retires the status, and any
                    // confirmation it was carrying with it.
                    self.clear_transient();
                    self.activate_tab(cell.index);
                }
            }
            _ => {}
        }
        true
    }

    /// The active tab.
    ///
    /// **Signature unchanged from when there was one editor**, which is what
    /// let this milestone's first task touch no test: seventy-six callers
    /// outside this file go through here and none of them need to know a list
    /// exists.
    pub fn editor(&self) -> &EditorPanel {
        &self.tabs[self.active].panel
    }

    pub fn editor_mut(&mut self) -> &mut EditorPanel {
        &mut self.tabs[self.active].panel
    }

    /// One open file by position, active or not.
    ///
    /// Separate from `editor()` because the two can differ and the difference is
    /// the whole point: a parse landing on a backgrounded tab is invisible
    /// through the active-tab accessor.
    pub fn tab(&self, index: usize) -> &EditorPanel {
        &self.tabs[index].panel
    }

    /// How many files are open. Never zero.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Index of the active tab.
    pub fn active_tab(&self) -> usize {
        self.active
    }
}
