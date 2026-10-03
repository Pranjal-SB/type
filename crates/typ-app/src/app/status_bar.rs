//! The status bar as a region: where each segment is drawn, and the keys and
//! clicks that reach the ones that do something (interface §4, §5).
//!
//! Segments are addressed by id (architecture, `status_segments()`), and
//! [`App::status_layout`] places them once for the paint, the hit-test and the
//! caret, so a click lands on the segment drawn under it.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use typ_core::KeyChord;

use crate::app::{App, Focus, SEGMENT_GAP};
use crate::status::{Segment, SegmentId};

/// The bar laid out: the left half as it fits, the gap, and each right-hand
/// segment with the cells it takes.
pub(super) struct StatusLayout {
    left: String,
    gap: usize,
    segments: Vec<(Segment, Rect)>,
}

impl App {
    /// Where everything on the status bar in `area` goes.
    pub(super) fn status_layout(&self, area: Rect) -> StatusLayout {
        let left = self.status_left();
        let right_segments = self.status_segments();
        // Measured in cells, not chars. A CJK file name is one char and two
        // cells per character, and counting chars pushed the right half's
        // tail off the end of the bar. Gap 105.
        let widths: Vec<usize> = right_segments
            .iter()
            .map(|s| typ_buffer::display_width(&s.text))
            .collect();
        let right_width: usize = widths.iter().sum::<usize>()
            + SEGMENT_GAP.len() * right_segments.len().saturating_sub(1);

        // The right half is the fixed cost; the left is truncated to whatever
        // is left over, so a long message never pushes the position off-screen.
        let width = area.width as usize;
        let room = width.saturating_sub(right_width + 2);
        let left = clip_to_cells(&left, room);
        let gap = width.saturating_sub(typ_buffer::display_width(&left) + right_width);

        let mut x = area.x as usize + typ_buffer::display_width(&left) + gap;
        let segments = right_segments
            .into_iter()
            .zip(widths)
            .map(|(segment, cells)| {
                let at = x.min(u16::MAX as usize) as u16;
                let rect = Rect::new(at, area.y, cells.min(u16::MAX as usize) as u16, 1)
                    .intersection(area);
                x += cells + SEGMENT_GAP.len();
                (segment, rect)
            })
            .collect();
        StatusLayout {
            left,
            gap,
            segments,
        }
    }

    pub(super) fn render_status(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let background = Style::default()
            .fg(self.theme.status_bar_fg)
            .bg(self.theme.status_bar_bg);
        let layout = self.status_layout(area);
        let focused = self.focused_segment();

        // Each segment carries its own emphasis. This is where
        // `status_bar_inactive_fg` and `status_bar_accent` earn their place:
        // without them the bar is one weight of text and a reader has to parse
        // it rather than glance at it. The one with the keys takes the primary
        // selection's ground (interface §4).
        let mut spans = vec![
            Span::styled(layout.left, background),
            Span::styled(" ".repeat(layout.gap), background),
        ];
        for (index, (segment, _)) in layout.segments.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(SEGMENT_GAP, background));
            }
            let style = if focused == Some(segment.id) {
                background
                    .fg(self.theme.selection_fg)
                    .bg(self.theme.selection_primary_bg)
            } else {
                background.fg(segment.emphasis.colour(&self.theme))
            };
            spans.push(Span::styled(segment.text.clone(), style));
        }

        Paragraph::new(Line::from(spans))
            .style(background)
            .render(area, buf);
    }

    /// The segments that do something, in the order they are drawn.
    fn actionable_segments(&self) -> Vec<SegmentId> {
        self.status_segments()
            .into_iter()
            .map(|s| s.id)
            .filter(|id| id.action().is_some())
            .collect()
    }

    /// The segment with the keys, while the status bar has them. Falls to
    /// the first that does something when the one it was on has gone, as the
    /// error count does once the last error is fixed.
    pub fn focused_segment(&self) -> Option<SegmentId> {
        if self.focus != Focus::Status {
            return None;
        }
        let actionable = self.actionable_segments();
        if actionable.contains(&self.segment) {
            return Some(self.segment);
        }
        actionable.first().copied()
    }

    /// Give the status bar the keys, on its first segment that does something.
    pub(super) fn focus_status(&mut self) {
        self.set_focus(Focus::Status);
        if let Some(first) = self.actionable_segments().first() {
            self.segment = *first;
        }
    }

    /// A key while the status bar has focus. `←→` move between the segments
    /// that do something, `enter` does it, `esc` goes back. Returns whether
    /// the bar took the key; anything else goes on to the keymap, which runs
    /// only what the app owns, so typing here edits nothing.
    pub(super) fn handle_status_chord(&mut self, chord: &KeyChord) -> Result<bool> {
        if !chord
            .raw
            .modifiers
            .difference(KeyModifiers::SHIFT)
            .is_empty()
        {
            return Ok(false);
        }
        let step: isize = match chord.raw.code {
            KeyCode::Left => -1,
            KeyCode::Right => 1,
            KeyCode::Enter => {
                if let Some(id) = self.focused_segment() {
                    self.activate_segment(id)?;
                }
                return Ok(true);
            }
            KeyCode::Esc => {
                self.focus_back();
                self.mark_dirty();
                return Ok(true);
            }
            _ => return Ok(false),
        };
        let actionable = self.actionable_segments();
        if let Some(at) = self
            .focused_segment()
            .and_then(|id| actionable.iter().position(|a| *a == id))
        {
            let next = (at as isize + step).clamp(0, actionable.len() as isize - 1);
            self.segment = actionable[next as usize];
            self.mark_dirty();
        }
        Ok(true)
    }

    /// Do what a segment does: the same for `enter` on it and a click.
    ///
    /// What it does is about the editor (a line to go to, a problem to jump
    /// to), so the keys go back to the pane first; except for the zoom,
    /// which is about whichever region is zoomed and has them already.
    fn activate_segment(&mut self, id: SegmentId) -> Result<()> {
        let Some(action) = id.action() else {
            return Ok(());
        };
        if id != SegmentId::Zoom {
            self.set_focus(Focus::Editor);
        }
        self.apply_named_action(action)
    }

    /// A press on the status bar. A segment that does something does it; the
    /// rest of the bar takes the press and does nothing. Returns whether the
    /// press was on the bar.
    pub fn press_status_at(&mut self, column: u16, row: u16, frame: Rect) -> Result<bool> {
        let (_, area) = crate::layout::split_frame(frame);
        if !area.contains((column, row).into()) {
            return Ok(false);
        }
        let hit = self
            .status_layout(area)
            .segments
            .into_iter()
            .find(|(_, rect)| rect.contains((column, row).into()))
            .map(|(segment, _)| segment.id);
        if let Some(id) = hit {
            self.activate_segment(id)?;
        }
        Ok(true)
    }

    /// Where the caret goes while the bar has the keys: the focused segment's
    /// first cell, so the hardware cursor is inside the focused region.
    pub(super) fn status_cursor(&self, area: Rect) -> Option<(u16, u16)> {
        let id = self.focused_segment()?;
        self.status_layout(area)
            .segments
            .into_iter()
            .find(|(segment, _)| segment.id == id)
            .map(|(_, rect)| (rect.x, rect.y))
    }
}

/// The longest prefix of `text` that fits in `room` cells, whole graphemes only.
fn clip_to_cells(text: &str, room: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut used = 0;
    text.graphemes(true)
        .take_while(|g| {
            used += typ_buffer::display_width(g);
            used <= room
        })
        .collect()
}
