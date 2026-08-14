use std::path::Path;

use anyhow::Result;
use ratatui::layout::Rect;
use typ_core::{Panel, PanelEvent, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;
use typ_panel_tree::TreePanel;
use typ_registry::Registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Editor,
}

pub struct App {
    tree: TreePanel,
    editor: EditorPanel,
    registry: Registry,
    theme: ThemeColors,
    focus: Focus,
    quit: bool,
}

impl App {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            tree: TreePanel::new(root)?,
            editor: EditorPanel::from_str(""),
            registry: Registry::with_builtins(),
            theme: ThemeColors::default(),
            focus: Focus::Tree,
            quit: false,
        })
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn focused_name(&self) -> &'static str {
        match self.focus {
            Focus::Tree => self.tree.name(),
            Focus::Editor => self.editor.name(),
        }
    }

    pub fn editor_title(&self) -> String {
        self.editor.title()
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Editor,
            Focus::Editor => Focus::Tree,
        };
    }

    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        // The registry decides the handler. There is one content panel today,
        // but the lookup runs from day one so adding viewers never touches this.
        let _handler = self.registry.handler_for(path);
        self.editor = EditorPanel::from_path(path)?;
        self.focus = Focus::Editor;
        Ok(())
    }

    /// Process events emitted by panels.
    pub fn apply(&mut self, events: Vec<PanelEvent>) -> Result<()> {
        for event in events {
            match event {
                PanelEvent::Quit => self.quit = true,
                PanelEvent::OpenFile { path, .. } | PanelEvent::OpenWith { path, .. } => {
                    self.open_path(&path)?;
                }
                // Redraw happens every loop pass in the walking skeleton.
                PanelEvent::NeedsRedraw => {}
                // Two fixed panels, so these are no-ops until the layout
                // system lands.
                PanelEvent::CloseSelf | PanelEvent::Focus(_) => {}
                PanelEvent::RunCommand { .. } | PanelEvent::Notify { .. } => {}
            }
        }
        Ok(())
    }

    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (tree_area, editor_area) = crate::layout::split(frame.area());
        let (w, h) = (frame.area().width, frame.area().height);

        let tree_ctx = RenderContext {
            theme: &self.theme,
            is_focused: self.focus == Focus::Tree,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };
        self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);

        let editor_ctx = RenderContext {
            theme: &self.theme,
            is_focused: self.focus == Focus::Editor,
            panel_index: 1,
            terminal_width: w,
            terminal_height: h,
        };
        self.editor
            .render(editor_area, frame.buffer_mut(), &editor_ctx);

        // Only the focused panel gets a cursor, and it is the terminal's real
        // one — set after drawing, so it lands on top of the frame. Panels with
        // nothing to edit return None and the cursor stays hidden.
        let focused_area = match self.focus {
            Focus::Tree => tree_area,
            Focus::Editor => editor_area,
        };
        if let Some((x, y)) = self.focused().cursor_position(focused_area) {
            frame.set_cursor_position((x, y));
        }
    }

    fn focused(&self) -> &dyn Panel {
        match self.focus {
            Focus::Tree => &self.tree,
            Focus::Editor => &self.editor,
        }
    }

    /// Areas for hit-testing mouse events, in the same order as `render`.
    pub fn areas(&self, area: Rect) -> (Rect, Rect) {
        crate::layout::split(area)
    }

    pub fn tree_mut(&mut self) -> &mut TreePanel {
        &mut self.tree
    }

    pub fn editor_mut(&mut self) -> &mut EditorPanel {
        &mut self.editor
    }

    pub fn focused_mut(&mut self) -> &mut dyn Panel {
        match self.focus {
            Focus::Tree => &mut self.tree,
            Focus::Editor => &mut self.editor,
        }
    }
}
