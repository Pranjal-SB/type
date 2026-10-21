//! Code actions (interface §8 2c): asked when the caret settles, shown as
//! `✦` in the gutter and after the line, listed by `ctrl+.`, a click on
//! either `✦` or the context menu over text.
//!
//! **Asked the way completion is**: at the end of a batch, one question in
//! flight, an answer for an older document or another caret dropped and the
//! caret asked about again. A server without `codeActionProvider` is never
//! asked. Choosing one applies its edit through `apply_workspace_edit` (one
//! undo per buffer) and then runs its command, whose own edit comes back as
//! `workspace/applyEdit`.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use typ_buffer::Position;
use typ_lsp::ServerId;

use super::{App, Focus};
use crate::float_menu::FloatMenu;
use crate::lsp::{Ask, CodeAction, Pending};

/// What the marks show (interface §9).
const MARK: &str = "\u{2726}";
/// `CodeActionTriggerKind.Automatic`: asked because the caret moved.
const AUTOMATIC: u8 = 2;

/// What is offered, what was last asked, and what the open list holds.
#[derive(Default)]
pub(crate) struct Fixing {
    offered: Option<Offered>,
    /// The document, caret and version last asked about. Asked again when
    /// any of the three differs.
    asked: Option<(PathBuf, Position, i32)>,
    /// What the open float's fix rows choose from, frozen when it opened so
    /// an answer arriving while it is up cannot move a row under the user.
    listed: Option<(ServerId, Vec<CodeAction>)>,
}

/// The actions a server offered for one line of one document.
struct Offered {
    path: PathBuf,
    line: usize,
    server: ServerId,
    actions: Vec<CodeAction>,
}

impl App {
    /// The titles of what is offered at the caret, if anything is.
    pub fn code_action_titles(&self) -> Option<Vec<String>> {
        let offered = self.offered_here()?;
        Some(offered.actions.iter().map(|a| a.title.clone()).collect())
    }

    /// The code actions the open context menu lists, if it is up.
    pub fn context_fixes(&self) -> Option<Vec<String>> {
        self.context.as_ref().map(FloatMenu::fixes)
    }

    /// What is offered, while the caret is on the line it was offered for.
    fn offered_here(&self) -> Option<&Offered> {
        let offered = self.fixing.offered.as_ref()?;
        let editor = self.shown_editor()?;
        (editor.path() == Some(offered.path.as_path()) && editor.cursor().line == offered.line)
            .then_some(offered)
    }

    /// The titles to lead the context menu over text with.
    pub(super) fn fix_titles(&self) -> Vec<String> {
        self.code_action_titles().unwrap_or_default()
    }

    /// Freeze what is offered for a float about to list it.
    pub(super) fn list_fixes(&mut self) {
        self.fixing.listed = self
            .offered_here()
            .map(|offered| (offered.server, offered.actions.clone()));
    }

    /// The end of a batch: drop what the caret has left, and ask about where
    /// it is now if that is not what was last asked and nothing is in flight.
    pub(super) fn settle_code_actions(&mut self) {
        if self.offered_here().is_none() {
            self.fixing.offered = None;
        }
        if self.focus != Focus::Editor || self.lsp.in_flight(Ask::CodeAction) {
            return;
        }
        let Some(editor) = self.shown_editor() else {
            return;
        };
        let Some(path) = editor.path().map(|p| p.to_path_buf()) else {
            return;
        };
        let Some(version) = self.lsp.version(&path) else {
            return;
        };
        let cursor = editor.cursor();
        let key = (path.clone(), cursor, version);
        if self.fixing.asked.as_ref() == Some(&key) {
            return;
        }
        let (start, end) = editor.selections().primary().range();
        let buffer = editor.buffer();
        let rope = buffer.snapshot();
        let (from, to) = (buffer.char_index(start), buffer.char_index(end));
        let under: Vec<typ_core::Diagnostic> = self
            .lsp
            .diagnostics(&path)
            .iter()
            .filter(|d| d.range.0.line <= cursor.line && cursor.line <= d.range.1.line)
            .cloned()
            .collect();
        let under: Vec<(usize, usize, typ_core::Diagnostic)> = under
            .into_iter()
            .map(|d| {
                (
                    buffer.char_index(d.range.0),
                    buffer.char_index(d.range.1),
                    d,
                )
            })
            .collect();
        self.fixing.asked = Some(key);
        // Silent when there is no answer to be had, as completion is: this
        // fires on every caret move.
        let asked = self
            .lsp
            .ask_with(Ask::CodeAction, &path, 0, |uri, encoding| {
                let at = |index| {
                    let p = typ_lsp::to_lsp(encoding, rope.slice(..), index);
                    serde_json::json!({ "line": p.line, "character": p.character })
                };
                let diagnostics: Vec<serde_json::Value> = under
                    .iter()
                    .map(|(from, to, d)| {
                        serde_json::json!({
                            "range": { "start": at(*from), "end": at(*to) },
                            "severity": severity_number(d.severity),
                            "message": d.message,
                            "source": d.source,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "range": { "start": at(from), "end": at(to) },
                    "context": { "diagnostics": diagnostics, "triggerKind": AUTOMATIC },
                })
            });
        if asked.is_ok() {
            self.lsp.stamp_last(cursor);
        }
    }

    /// An answer to a code action question. Returns whether the screen
    /// changed.
    pub(super) fn code_actions_answered(
        &mut self,
        pending: Pending,
        result: serde_json::Value,
    ) -> bool {
        let (path, at) = pending.asked_at;
        let current = self.lsp.version(&path) == Some(pending.version)
            && self.focus == Focus::Editor
            && self
                .shown_editor()
                .is_some_and(|e| e.path() == Some(path.as_path()) && e.cursor() == at);
        if !current {
            // Asked again about wherever the caret is by the end of this
            // batch.
            self.fixing.asked = None;
            return false;
        }
        let had = self.fixing.offered.is_some();
        let actions = crate::lsp::code_actions(&result);
        self.fixing.offered = (!actions.is_empty()).then_some(Offered {
            path,
            line: at.line,
            server: pending.server,
            actions,
        });
        had || self.fixing.offered.is_some()
    }

    /// Ask again at the end of the batch: the server cancelled its answer.
    pub(super) fn code_actions_again(&mut self) {
        self.fixing.asked = None;
    }

    /// `ctrl+.`: what is offered at the caret, as a float hung from it.
    pub(super) fn open_code_actions(&mut self) {
        let anchor = self.caret_cell().unwrap_or((self.screen.x, self.screen.y));
        self.open_code_actions_at(anchor);
    }

    fn open_code_actions_at(&mut self, anchor: (u16, u16)) {
        let titles = self.fix_titles();
        if titles.is_empty() {
            self.status = Some(
                crate::lsp::NoAnswer::Empty(Ask::CodeAction)
                    .message()
                    .to_string(),
            );
            return;
        }
        self.list_fixes();
        self.context = FloatMenu::with_fixes(&titles, &[], &self.keymap, anchor);
        self.mark_dirty();
    }

    /// The fix at `index` in the list that was open: its edit, then its
    /// command.
    pub(super) fn choose_code_action(&mut self, index: usize) {
        let Some((server, action)) = self
            .fixing
            .listed
            .take()
            .and_then(|(server, actions)| Some((server, actions.into_iter().nth(index)?)))
        else {
            return;
        };
        if let Some(edit) = &action.edit {
            let encoding = self.lsp.encoding(server);
            if !self.apply_workspace_edit(edit, encoding) {
                return;
            }
        }
        match &action.command {
            Some(command) => self.lsp.execute_command(server, command),
            None if action.edit.is_none() => {
                self.status = Some(format!("{} did nothing TYPE can apply.", action.title));
            }
            None => {}
        }
    }

    /// Where the caret is drawn, in the last frame's geometry.
    fn caret_cell(&self) -> Option<(u16, u16)> {
        let content = self
            .current_pane_area(self.editor_region(self.screen))
            .content;
        self.panes[self.pane]
            .shown()
            .panel()
            .cursor_position(content)
    }

    /// Where the two `✦` go in `frame`: the gutter's last cell on the line,
    /// and the cell after the line's end, each only when it is on screen.
    /// The one geometry the paint, the click and `targets` share.
    pub(super) fn code_action_marks(&self, frame: Rect) -> Vec<Rect> {
        let Some(offered) = self.offered_here() else {
            return Vec::new();
        };
        let Some(editor) = self.shown_editor() else {
            return Vec::new();
        };
        let content = self.current_pane_area(self.editor_region(frame)).content;
        let text = editor.text_area(content);
        let line = offered.line;
        let Some(row) = line.checked_sub(editor.top_line()) else {
            return Vec::new();
        };
        let Ok(row) = u16::try_from(row) else {
            return Vec::new();
        };
        if row >= text.height {
            return Vec::new();
        }
        let mut marks = Vec::new();
        if text.x > content.x {
            marks.push(Rect::new(text.x - 1, text.y + row, 1, 1));
        }
        let end = Position {
            line,
            col: editor.buffer().line_grapheme_count(line),
        };
        if let Some((x, y)) = editor.cell_of(end, content)
            && x + 1 < text.right()
        {
            marks.push(Rect::new(x + 1, y, 1, 1));
        }
        marks
    }

    /// A left press on a `✦` opens the list there. Returns whether it was
    /// on one.
    pub fn press_code_action_at(&mut self, column: u16, row: u16, frame: Rect) -> bool {
        let Some(mark) = self
            .code_action_marks(frame)
            .into_iter()
            .find(|mark| mark.contains((column, row).into()))
        else {
            return false;
        };
        self.clear_transient();
        self.set_focus(Focus::Editor);
        self.open_code_actions_at((mark.x, mark.y));
        true
    }

    /// The marks, over the text the pane drew.
    pub(super) fn render_code_action_marks(&self, frame: Rect, buf: &mut Buffer) {
        let style = Style::default().fg(self.theme.status_bar_accent);
        for mark in self.code_action_marks(frame) {
            if buf.area.contains((mark.x, mark.y).into()) {
                buf[(mark.x, mark.y)].set_symbol(MARK).set_style(style);
            }
        }
    }
}

/// `DiagnosticSeverity`, as the protocol numbers it.
fn severity_number(severity: typ_core::Severity) -> u8 {
    match severity {
        typ_core::Severity::Error => 1,
        typ_core::Severity::Warning => 2,
        typ_core::Severity::Information => 3,
        typ_core::Severity::Hint => 4,
    }
}
