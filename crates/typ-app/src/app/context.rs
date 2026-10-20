//! The context menu (interface §5, §8 4b): `shift+f10`, the menu key,
//! `ctrl+k .` or a right-click.
//!
//! What it lists is decided by what it was opened on, as a set of `Group`s
//! (plan decision 4): a tab, its tabs; text, the editing groups; a tree row,
//! files; a pane's number or zoom, panes; the dock's heading, panels. A
//! right-click on a tab or a tree row makes it current first, so "close the
//! tab" closes the one clicked.

use anyhow::Result;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Action, Group, KeyChord, Panel, RenderContext};

use super::{App, Focus};
use crate::app::targets::TargetKind;
use crate::float_menu::{FloatMenu, Outcome};

/// What text gets.
const TEXT: &[Group] = &[Group::Edit, Group::Selection, Group::Find, Group::Code];
/// Said when the focused region has no menu.
const NOTHING: &str = "Nothing here has a context menu.";

impl App {
    /// The actions the context menu shows, while it is up.
    pub fn context_rows(&self) -> Option<Vec<Action>> {
        self.context.as_ref().map(FloatMenu::actions)
    }

    /// From the keyboard: for the focused region's current thing, hung from
    /// the caret or the selected tree row.
    pub(super) fn open_context_menu(&mut self) {
        let frame = self.screen;
        match self.focus {
            Focus::Editor => {
                let content = self.current_pane_area(self.editor_region(frame)).content;
                let anchor = self.panes[self.pane]
                    .shown()
                    .panel
                    .cursor_position(content)
                    .unwrap_or((content.x, content.y));
                self.show_context(TEXT, anchor);
            }
            Focus::Tree => {
                let selected = TargetKind::TreeRow(self.tree.selected_index());
                let anchor = self
                    .targets(frame)
                    .into_iter()
                    .find(|t| t.kind == selected)
                    .map_or((frame.x, frame.y), |t| t.point);
                self.show_context(&[Group::Files], anchor);
            }
            Focus::Status => self.status = Some(NOTHING.to_string()),
        }
    }

    /// A right press at a cell: what is there becomes current, and the menu
    /// for it opens at the pointer.
    pub fn open_context_at(&mut self, column: u16, row: u16, frame: Rect) {
        self.clear_transient();
        let at = (column, row);
        let hit = self
            .targets(frame)
            .into_iter()
            .find(|t| t.rect.contains(at.into()))
            .map(|t| t.kind);
        let groups: &[Group] = match hit {
            Some(TargetKind::Tab { pane, tab }) => {
                self.make_current(pane);
                self.activate_tab(tab);
                &[Group::Tabs]
            }
            Some(TargetKind::PaneNumber(pane) | TargetKind::PaneZoom(pane)) => {
                self.make_current(pane);
                &[Group::Panes]
            }
            Some(TargetKind::TreeRow(entry)) => {
                self.tree.select(entry);
                self.set_focus(Focus::Tree);
                &[Group::Files]
            }
            Some(TargetKind::DockHeading | TargetKind::DockZoom | TargetKind::DockFolded) => {
                &[Group::Panels]
            }
            Some(
                TargetKind::Segment(_)
                | TargetKind::MenuRow(_)
                | TargetKind::FloatRow(_)
                | TargetKind::BarGroup(_),
            ) => return,
            None => {
                let editor = self.editor_region(frame);
                let Some(pane) = self
                    .pane_areas(editor)
                    .into_iter()
                    .find(|area| area.content.contains(at.into()))
                else {
                    return;
                };
                self.make_current(pane.index);
                TEXT
            }
        };
        self.show_context(groups, at);
    }

    fn show_context(&mut self, groups: &[Group], anchor: (u16, u16)) {
        self.context = FloatMenu::new(groups, &self.keymap, anchor);
        if self.context.is_none() {
            self.status = Some(NOTHING.to_string());
        }
        self.mark_dirty();
    }

    /// A key while the menu is up: `FloatMenu::key` says what it did.
    pub(super) fn handle_context_chord(&mut self, chord: &KeyChord) -> Result<()> {
        let Some(mut menu) = self.context.take() else {
            return Ok(());
        };
        self.mark_dirty();
        match menu.key(chord) {
            Outcome::Stay => self.context = Some(menu),
            Outcome::Close => {}
            Outcome::Run(action) => self.apply_named_action(action)?,
        }
        Ok(())
    }

    /// A mouse event while the menu is up. A left press on a row runs it, any
    /// other press closes the menu and is not also a click on what is
    /// behind. Returns whether the menu took the event.
    pub fn route_context_mouse(&mut self, event: MouseEvent, frame: Rect) -> Result<bool> {
        if self.context.is_none() {
            return Ok(false);
        }
        if !matches!(event.kind, MouseEventKind::Down(_)) {
            return Ok(true);
        }
        let Some(mut menu) = self.context.take() else {
            return Ok(true);
        };
        self.mark_dirty();
        let (body, _) = crate::layout::split_frame(frame);
        if event.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(action) = menu.click(body, event.column, event.row)
        {
            self.apply_named_action(action)?;
        }
        Ok(true)
    }

    pub(super) fn render_context(&self, frame: Rect, buf: &mut Buffer) {
        let Some(menu) = &self.context else {
            return;
        };
        let (body, _) = crate::layout::split_frame(frame);
        menu.render(body, buf, &self.float_context(frame));
    }

    /// What a float that has the keyboard renders with.
    pub(super) fn float_context(&self, frame: Rect) -> RenderContext<'_> {
        RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            diagnostics: &[],
            // It has the keyboard, so its border says so.
            is_focused: true,
            panel_index: 3,
            terminal_width: frame.width,
            terminal_height: frame.height,
        }
    }
}
