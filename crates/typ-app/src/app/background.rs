//! What arrives about an open tab from off the render thread: a finished
//! parse, a write to its file by something else, a watch that failed.
//!
//! A child module of `app`, split out at M4 with `servers`, so `app.rs` keeps
//! state, focus and dispatch and nothing that answers a worker.

use std::path::Path;

use anyhow::Result;

use super::{App, Pane};

impl App {
    /// Ask for a parse if the buffer has changed since the last request.
    ///
    /// Called from three places, and the third is the one that is easy to miss
    /// reading only "after an edit": a buffer opened, an edit, and M2.4's
    /// external-change reload, which replaces the text without going through
    /// an `Action` at all. Missing that one leaves a file edited outside TYPE
    /// showing the previous version's highlights indefinitely.
    ///
    /// Cheap enough to call on every event loop pass: a `u64` comparison, and
    /// nothing at all for a buffer whose extension has no grammar.
    ///
    /// **Every pane's visible tab, not only the current pane's.** A file shown
    /// in a pane without focus is still on screen, and an outside write
    /// reloading it is as much a reason to re-parse as a keystroke is.
    pub(crate) fn request_parse_if_stale(&mut self) {
        for pane in 0..self.panes.len() {
            self.request_parse_for(pane);
        }
    }

    fn request_parse_for(&mut self, pane: usize) {
        let tab = self.panes[pane].shown_mut();
        let Some(language) = tab.panel.language() else {
            return;
        };
        let revision = tab.panel.buffer().revision();
        if tab.parsed_revision == Some(revision) {
            return;
        }
        let Some(worker) = &mut self.parse_worker else {
            return;
        };

        let Some(generation) = worker.request(language, tab.panel.buffer().snapshot()) else {
            // The thread is gone: a panic compiling a grammar's queries is
            // how. Recording a request that was never delivered is what left
            // highlighting dead while `is_wired` still said yes. Gap 91.
            self.parse_worker = None;
            crate::log_error!("the parse worker is gone; syntax highlighting is off");
            self.status = Some("Syntax highlighting stopped: its worker thread died.".into());
            return;
        };
        tab.parsed_revision = Some(revision);
        tab.awaited_generation = Some(generation);
    }

    /// A completed parse arrived. Returns whether the screen changed.
    ///
    /// The generation says which *request* this answers, and every tab records
    /// the one it is waiting for, so the result goes to the tab that asked for
    /// it, which is not always the active one.
    ///
    /// Applying it to a backgrounded tab is not a nicety. That tab already
    /// recorded the revision as requested, so throwing the answer away would
    /// leave the buffer unhighlighted for as long as it stays open: nothing
    /// would ever ask again.
    pub fn handle_parsed(&mut self, parsed: typ_syntax::Parsed) -> bool {
        // **Anything older will never come.** The worker answers in generation
        // order and drops whatever is queued behind the job it is running,
        // which with tabs includes other tabs' requests. A tab still waiting on
        // an older generation is forgotten, so the next pass that finds it
        // active asks again rather than trusting a revision nobody parsed.
        // Gap 111.
        for tab in self.all_tabs_mut() {
            if tab
                .awaited_generation
                .is_some_and(|g| g < parsed.generation)
            {
                tab.awaited_generation = None;
                tab.parsed_revision = None;
            }
        }

        // Not a parse anyone is still waiting for. It describes a buffer that
        // has since been replaced, and its byte offsets index text that is gone.
        let Some((pane, index)) =
            self.find_tab(|tab| tab.awaited_generation == Some(parsed.generation))
        else {
            return false;
        };
        let tab = &mut self.panes[pane].tabs[index];
        tab.awaited_generation = None;
        tab.panel.set_syntax(parsed.generation, parsed.syntax);
        // Only a tree for a buffer on screen changes what is painted.
        index == self.panes[pane].active
    }

    /// Watch the file in tab `index` of `pane`, if it has one and nothing is
    /// watching it yet.
    ///
    /// A failure here is not worth interrupting anyone over: the editor keeps
    /// working, it just stops noticing outside writes. It goes to the log,
    /// which is where the answer will be looked for.
    pub(super) fn watch_tab(&mut self, pane: usize, index: usize) {
        let tab = &mut self.panes[pane].tabs[index];
        if tab.watch.is_some() {
            return;
        }
        let (Some(sender), Some(path)) = (self.sender.clone(), tab.panel.path()) else {
            return;
        };
        let path = path.to_path_buf();
        match typ_buffer::watch_file(&path, move |changed, what| {
            let event = match what {
                typ_buffer::WatchEvent::Changed => typ_core::AppEvent::FileChanged(changed),
                typ_buffer::WatchEvent::Failed(reason) => typ_core::AppEvent::WatchFailed {
                    path: changed,
                    reason,
                },
            };
            let _ = sender.send(event);
        }) {
            Ok(watch) => tab.watch = Some(watch),
            Err(e) => crate::log_warn!("watching {} failed: {e:#}", path.display()),
        }
    }

    /// Say that a tab's file changed under unsaved work.
    fn warn_changed_on_disk(&mut self, pane: usize, index: usize) {
        self.panes[pane].tabs[index].changed_on_disk = false;
        self.status = Some(format!(
            "{} changed on disk. Your unsaved changes are kept; Ctrl+S overwrites it.",
            self.panes[pane].tabs[index].panel.file_name()
        ));
    }

    /// A tab that became active may have been written to while it was not.
    pub(super) fn warn_if_changed_on_disk(&mut self) {
        if self.panes[self.pane].shown().changed_on_disk {
            self.warn_changed_on_disk(self.pane, self.panes[self.pane].active);
        }
    }

    /// The file changed on disk. Three states, one of them automatic.
    ///
    /// Reloading a dirty buffer discards the user's edits; ignoring the change
    /// discards the other writer's on the next save. Only the user knows which
    /// matters, so the only thing to do is say so and touch nothing.
    /// Returns whether anything on screen changed, so the loop can decline to
    /// repaint for a watcher event that turned out to be our own save.
    ///
    /// **Any tab, not only the active one.** A clean background tab reloads in
    /// place; a dirty one is flagged and warns when it is switched to. Gap 85.
    pub fn handle_external_change(&mut self, path: &Path) -> Result<bool> {
        let Some((pane, index)) = self.find_tab(|tab| tab.panel.path() == Some(path)) else {
            return Ok(false);
        };
        let on_screen = index == self.panes[pane].active;

        if !path.exists() {
            self.status = Some(format!(
                "{} was deleted on disk. Ctrl+S writes it back.",
                self.panes[pane].tabs[index].panel.file_name()
            ));
            return Ok(true);
        }

        // Covers our own save: the watcher reports the write, and what is on
        // disk is what we have, so there is nothing to do.
        if self.panes[pane].tabs[index].panel.matches_disk() {
            return Ok(false);
        }

        if self.panes[pane].tabs[index].panel.is_dirty() {
            if on_screen {
                self.warn_changed_on_disk(pane, index);
            } else {
                self.panes[pane].tabs[index].changed_on_disk = true;
            }
            return Ok(on_screen);
        }

        // **Not `?`.** `reload` reads through `read_to_string`, so a build step
        // or a `git checkout` that leaves non-UTF-8 bytes at the path fails
        // here, and propagating that ends the process from `step_batch`,
        // discarding every *other* tab's unsaved work on a watcher event rather
        // than on anything the user did. Gap 70.
        if let Err(e) = self.panes[pane].tabs[index].panel.reload() {
            self.status = Some(format!(
                "Could not reload {}: {e:#}",
                self.panes[pane].tabs[index].panel.file_name()
            ));
            return Ok(true);
        }
        // `reload` swaps in a fresh `TextBuffer`, so revisions restart and the
        // comparison in `request_parse_if_stale` would be against a number
        // from a buffer that no longer exists. A background tab is parsed
        // when it is next activated.
        self.panes[pane].tabs[index].parsed_revision = None;
        self.request_parse_if_stale();
        Ok(on_screen)
    }

    /// The watch on a file failed. Say so: until it is re-established an
    /// outside write goes unnoticed, and Ctrl+S would overwrite it.
    pub fn handle_watch_failure(&mut self, path: &Path, reason: &str) -> bool {
        // Only a file on screen, in whichever pane: one behind a tab is said
        // when it next matters, which is its next outside write.
        let Some(shown) = self
            .panes
            .iter()
            .map(Pane::shown)
            .find(|tab| tab.panel.path() == Some(path))
        else {
            return false;
        };
        self.status = Some(format!(
            "No longer watching {} for outside changes: {reason}",
            shown.panel.file_name()
        ));
        true
    }
}
