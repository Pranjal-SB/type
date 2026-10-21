//! The completion list on screen: its keys, its clicks, its paint, and the
//! ghost text after the caret (interface §8 2c).
//!
//! **Non-modal.** The list takes `tab`, `enter`, the arrows and `esc` while it
//! is up, and every other key is still the editor's, so typing carries on and
//! narrows it. A press anywhere but a row puts it away and is still a click on
//! what is under it.

use anyhow::Result;
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use typ_core::{Action, KeyChord};

use super::targets::{Target, TargetKind};
use super::{App, Focus};
use crate::completion::Areas;

/// The status bar's left half while the list is up.
const KEYS: &str = "tab accepts  \u{b7}  \u{2191}\u{2193} choose  \u{b7}  esc closes";

impl App {
    /// A key while the list is up: `tab` or `enter` accept, the arrows choose,
    /// `esc` closes. Returns whether the list took it.
    pub(super) fn handle_completion_chord(&mut self, chord: &KeyChord) -> Result<bool> {
        if self.completing.list.is_none() || self.focus != Focus::Editor {
            return Ok(false);
        }
        // `shift+tab` is still outdent, `ctrl+enter` still whatever it is.
        if !chord.raw.modifiers.is_empty() {
            return Ok(false);
        }
        match chord.raw.code {
            KeyCode::Tab | KeyCode::Enter => self.apply_named_action(Action::AcceptCompletion)?,
            KeyCode::Esc => self.close_completion(),
            KeyCode::Up | KeyCode::Down => {
                let delta = if chord.raw.code == KeyCode::Up { -1 } else { 1 };
                if let Some(list) = self.completing.list.as_mut() {
                    list.move_selection(delta);
                }
            }
            _ => return Ok(false),
        }
        self.mark_dirty();
        Ok(true)
    }

    /// `Action::AcceptCompletion`: the chosen item over the word typed so far,
    /// through the editor as one undo step (plan decision 2).
    ///
    /// From the server's `textEdit` start when it gave one on the caret's line,
    /// so a server that replaces more than the word (a `self.` it rewrites) is
    /// taken at its word.
    pub(super) fn accept_completion(&mut self) {
        let chosen = self
            .completion_query()
            .and(self.completing.list.as_ref())
            .and_then(|list| Some((list.selected_item()?.clone(), list.start)));
        let Some((item, word)) = chosen else {
            self.status = Some("No completion to accept.".to_string());
            return;
        };
        self.close_completion();
        let Some(editor) = self.shown_editor_mut() else {
            return;
        };
        let cursor = editor.cursor();
        let start = item
            .edit_start
            .filter(|s| s.line == cursor.line && s.col <= cursor.col)
            .unwrap_or(word);
        editor.complete(cursor.col - start.col, &item.text);
    }

    /// The keys the bar shows while the list is up.
    pub(super) fn completion_hint(&self) -> Option<&'static str> {
        self.completing.list.as_ref().map(|_| KEYS)
    }

    /// Where the list goes in `frame`, the row it hangs from, and the right
    /// edge of the text the ghost shifts along. The one geometry the paint,
    /// the click and `targets` share.
    fn completion_areas(&self, frame: Rect) -> Option<(Areas, (u16, u16), u16)> {
        let list = self.completing.list.as_ref()?;
        let typed = self.completion_query()?;
        let content = self.current_pane_area(self.editor_region(frame)).content;
        let caret = self.panes[self.pane]
            .shown()
            .panel()
            .cursor_position(content)?;
        let width = u16::try_from(typ_buffer::display_width(&typed)).unwrap_or(u16::MAX);
        let word_x = caret.0.saturating_sub(width);
        let (body, _) = crate::layout::split_frame(frame);
        Some((
            list.layout(body, frame.width, caret, word_x),
            caret,
            content.right(),
        ))
    }

    /// The list's rows as mouse targets, while it is up.
    pub(super) fn completion_targets(&self, frame: Rect) -> Vec<Target> {
        let (Some(list), Some((areas, _, _))) =
            (self.completing.list.as_ref(), self.completion_areas(frame))
        else {
            return Vec::new();
        };
        list.row_rects(&areas)
            .into_iter()
            .map(|(place, rect)| Target {
                rect,
                point: (rect.x, rect.y),
                kind: TargetKind::CompletionRow(place),
            })
            .collect()
    }

    /// A mouse event while the list is up. A left press on a row accepts it;
    /// any other press puts the list away and goes on to what is under it.
    /// Returns whether the list took the event.
    pub fn route_completion_mouse(&mut self, event: MouseEvent, frame: Rect) -> Result<bool> {
        let MouseEventKind::Down(button) = event.kind else {
            return Ok(false);
        };
        let Some((areas, _, _)) = self.completion_areas(frame) else {
            return Ok(false);
        };
        let hit = self
            .completing
            .list
            .as_ref()
            .and_then(|list| list.hit(&areas, event.column, event.row));
        self.mark_dirty();
        match hit {
            Some(place) if button == MouseButton::Left => {
                if let Some(list) = self.completing.list.as_mut() {
                    list.select(place);
                }
                self.apply_named_action(Action::AcceptCompletion)?;
                Ok(true)
            }
            _ => {
                let inside = areas.list.contains((event.column, event.row).into());
                self.close_completion();
                Ok(inside)
            }
        }
    }

    /// The ghost after the caret, then the list and its docs column.
    pub(super) fn render_completion(&self, frame: Rect, buf: &mut Buffer) {
        let Some(list) = self.completing.list.as_ref() else {
            return;
        };
        let Some((areas, caret, row_end)) = self.completion_areas(frame) else {
            return;
        };
        if let Some(ghost) = list.ghost() {
            paint_ghost(buf, caret, row_end, ghost, self.theme.receded_fg);
        }
        list.render(&areas, buf, &self.float_context(frame));
    }
}

/// Draw `ghost` at the caret in `fg`, moving the rest of the row right to
/// make room, so the text after the caret is pushed along rather than hidden.
fn paint_ghost(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    row_end: u16,
    ghost: &str,
    fg: ratatui::style::Color,
) {
    let row_end = row_end.min(buf.area.right());
    if x >= row_end || !buf.area.contains((x, y).into()) {
        return;
    }
    let width = u16::try_from(typ_buffer::display_width(ghost)).unwrap_or(u16::MAX);
    for col in (x..row_end.saturating_sub(width)).rev() {
        let cell = buf[(col, y)].clone();
        buf[(col + width, y)] = cell;
    }
    let bg = buf[(x, y)].bg;
    let room = usize::from(row_end - x);
    buf.set_stringn(x, y, ghost, room, Style::default().fg(fg).bg(bg));
}
