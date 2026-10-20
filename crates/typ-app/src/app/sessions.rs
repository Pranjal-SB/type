//! The app as a [`Session`] and back: what quitting writes down and what
//! starting again on the same project reads.

use std::path::Path;

use typ_buffer::Position;

use crate::app::{App, Focus, Pane, Tab};
use crate::session::{SavedPane, SavedTab, Session};

impl App {
    /// The layout as it is now. Untitled buffers have no path to come back
    /// to, so they are left out.
    pub fn session(&self) -> Session {
        let panes = self
            .panes
            .iter()
            .map(|pane| {
                let mut active = 0;
                let mut tabs = Vec::new();
                for (index, tab) in pane.tabs.iter().enumerate() {
                    // A view is not a file to come back to either.
                    let Some((editor, path)) = tab
                        .editor()
                        .and_then(|editor| Some((editor, editor.path()?)))
                    else {
                        continue;
                    };
                    if index == pane.active {
                        active = tabs.len();
                    }
                    let cursor = editor.cursor();
                    tabs.push(SavedTab {
                        path: path.to_path_buf(),
                        line: cursor.line,
                        col: cursor.col,
                        top: editor.top_line(),
                    });
                }
                SavedPane {
                    id: pane.id,
                    tabs,
                    active,
                }
            })
            .collect();
        Session {
            layout: self.layout.clone(),
            panes,
            current: self.panes[self.pane].id,
            preset: self.preset,
            picked: self.layout_chosen,
            dock_open: self.dock.open,
            dock_size: self.dock.size,
        }
    }

    /// Read the session at `path` and put the app in it. Returns what the
    /// status bar should say: a session that will not parse, or files that
    /// have gone. Never fails: a broken session starts clean.
    pub fn restore_session(&mut self, path: &Path) -> Option<String> {
        match crate::session::load(path) {
            Ok(None) => None,
            Ok(Some(session)) => self.restore(session),
            Err(e) => {
                crate::log_warn!("session ignored: {e}");
                Some(format!("session ignored, starting clean: {e}"))
            }
        }
    }

    /// Where `save_session` writes on quit. `None`, the default, writes
    /// nothing: `typ <file>` neither reads nor writes one (invariant 10).
    pub fn set_session_file(&mut self, path: Option<std::path::PathBuf>) {
        self.session_file = path;
    }

    /// Write the session, if this run has a session file. A failure is
    /// logged: the editor is exiting and there is nobody to tell.
    pub fn save_session(&self) {
        let Some(path) = &self.session_file else {
            return;
        };
        if let Err(e) = crate::session::save(path, &self.session()) {
            crate::log_warn!("saving the session to {} failed: {e}", path.display());
        }
    }

    fn restore(&mut self, session: Session) -> Option<String> {
        let mut gone: Vec<String> = Vec::new();
        let mut panes = Vec::new();
        for saved in session.panes {
            let mut pane = Pane::new(saved.id);
            let mut tabs = Vec::new();
            let mut active = 0;
            for (index, tab) in saved.tabs.iter().enumerate() {
                if !tab.path.exists() {
                    gone.push(tab.path.display().to_string());
                    continue;
                }
                let mut panel = match self.panel_for(&tab.path) {
                    Ok(panel) => panel,
                    Err(e) => {
                        gone.push(format!("{} ({e})", tab.path.display()));
                        continue;
                    }
                };
                let at = Position {
                    line: tab.line,
                    col: tab.col,
                };
                panel.restore_view(at, tab.top);
                if index <= saved.active {
                    active = tabs.len();
                }
                tabs.push(Tab::new(panel));
            }
            if !tabs.is_empty() {
                pane.tabs = tabs;
                pane.active = active;
            }
            panes.push(pane);
        }
        self.next_pane = panes.iter().map(|p| p.id.0 + 1).max().unwrap_or(1);
        self.panes = panes;
        self.layout = session.layout;
        self.dock.size = session.dock_size;
        self.set_preset(session.preset);
        self.dock.open = session.dock_open;
        self.layout_chosen = session.picked;
        // Each pane's tab settles as it would on opening: the configured
        // indent, a parse request. The current pane last, so it has the keys.
        let current = self.index_of(session.current).unwrap_or(0);
        for pane in (0..self.panes.len())
            .filter(|p| *p != current)
            .chain([current])
        {
            self.pane = pane;
            self.settle_active_tab();
        }
        self.set_focus(Focus::Editor);
        self.came_from.clear();
        self.mark_dirty();
        (!gone.is_empty()).then(|| format!("session: no longer there: {}", gone.join(", ")))
    }
}
