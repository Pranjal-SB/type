use std::any::Any;

use crossterm::event::MouseEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::{KeyChord, PanelEvent};

/// The colors a panel is allowed to know about.
///
/// Deliberately a small copy rather than a reference to a full theme: panels
/// should not be able to reach into application state through their theme.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub fg: Color,
    pub bg: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub border: Color,
    pub border_focused: Color,
    pub line_numbers: Color,
    pub cursor: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            fg: Color::White,
            bg: Color::Black,
            selection_bg: Color::Blue,
            selection_fg: Color::White,
            border: Color::DarkGray,
            border_focused: Color::Cyan,
            line_numbers: Color::DarkGray,
            cursor: Color::Yellow,
            status_bar_bg: Color::DarkGray,
            status_bar_fg: Color::White,
        }
    }
}

/// Everything a panel may see at render time.
///
/// This is the whole surface — a panel never receives `&AppState`.
pub struct RenderContext<'a> {
    pub theme: &'a ThemeColors,
    pub is_focused: bool,
    pub panel_index: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
}

/// A rectangular, focusable unit of UI.
///
/// Implementors provide five methods; everything else has a default. Panels
/// communicate outward by returning events, never by mutating shared state.
pub trait Panel: Any {
    /// Stable type name, used for registry lookup and session records.
    fn name(&self) -> &'static str;

    /// Dynamic title shown in the panel header.
    fn title(&self) -> String;

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext);

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent>;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// `panel_area` is supplied so the panel can translate to local coordinates.
    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        let _ = (event, panel_area);
        Vec::new()
    }

    /// Coalesced scroll. Positive is down.
    fn handle_scroll(&mut self, delta: i32, panel_area: Rect) -> Vec<PanelEvent> {
        let _ = (delta, panel_area);
        Vec::new()
    }

    /// Where the terminal cursor belongs, in screen coordinates, when this
    /// panel holds focus. `None` hides it.
    ///
    /// The app draws the real terminal cursor rather than a styled cell, so it
    /// blinks and reshapes the way every other terminal program's does. A panel
    /// with nothing to edit — a file tree, a viewer — leaves this defaulted.
    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        let _ = panel_area;
        None
    }

    /// Perform a named action.
    ///
    /// This is the only way a binding, the command palette, or the vim layer
    /// reaches a panel's behavior.
    ///
    /// `None` means "I do not handle this action" and lets the app try it.
    /// `Some(vec![])` means "handled, nothing to report" — a real outcome, as
    /// when adding a cursor at the edge of the document does nothing. Folding
    /// those two answers into an empty vector reads fine today and becomes a
    /// silent bug the first time an action needs both a panel implementation
    /// and an app fallback.
    fn apply_action(&mut self, action: crate::Action) -> Option<Vec<PanelEvent>> {
        let _ = action;
        None
    }

    /// Periodic hook for background work.
    fn tick(&mut self) -> Vec<PanelEvent> {
        Vec::new()
    }

    /// True when the panel consumes Escape itself (e.g. an open search box).
    fn captures_escape(&self) -> bool {
        false
    }

    /// `Some(message)` blocks closing until confirmed.
    fn needs_close_confirmation(&self) -> Option<String> {
        None
    }
}
