use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext, ThemeColors};

/// A panel implementing only the required methods proves the defaults work.
struct Minimal;

impl Panel for Minimal {
    fn name(&self) -> &'static str {
        "minimal"
    }
    fn title(&self) -> String {
        "Minimal".into()
    }
    fn render(&mut self, _area: Rect, _buf: &mut Buffer, _ctx: &RenderContext) {}
    fn handle_key(&mut self, _chord: KeyChord) -> Vec<PanelEvent> {
        vec![PanelEvent::NeedsRedraw]
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[test]
fn a_panel_needs_only_the_required_methods() {
    let mut p = Minimal;
    let chord = KeyChord::from_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(p.handle_key(chord), vec![PanelEvent::NeedsRedraw]);
}

#[test]
fn defaulted_methods_return_empty() {
    let mut p = Minimal;
    assert!(p.handle_scroll(3, Rect::new(0, 0, 10, 10)).is_empty());
    assert!(p.tick().is_empty());
    assert!(!p.captures_escape());
    assert!(p.needs_close_confirmation().is_none());
}

#[test]
fn a_panel_hides_the_cursor_unless_it_asks_for_one() {
    let p = Minimal;
    assert!(p.cursor_position(Rect::new(0, 0, 10, 10)).is_none());
}

#[test]
fn panels_are_dispatchable_as_trait_objects() {
    let panels: Vec<Box<dyn Panel>> = vec![Box::new(Minimal)];
    assert_eq!(panels[0].name(), "minimal");
    let _ = ThemeColors::default();
}
