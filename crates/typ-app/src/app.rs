use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use typ_core::{Action, KeyChord, Keymap, Panel, PanelEvent, RenderContext, ThemeColors};
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
    keymap: Keymap,
    theme: ThemeColors,
    focus: Focus,
    quit: bool,
    /// Message shown in the status bar until the next keypress.
    status: Option<String>,
    /// A quit was refused because a panel had something to confirm. The next
    /// quit goes through.
    quit_pending: bool,
}

/// Shown when there is nothing more urgent to say. Discoverability is part of
/// the product: bindings nobody can find are bindings that do not exist.
const HINT: &str = "Tab focus  ·  Enter open  ·  Ctrl+S save  ·  Ctrl+Q quit";

impl App {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            tree: TreePanel::new(root)?,
            editor: EditorPanel::from_str(""),
            registry: Registry::with_builtins(),
            keymap: Keymap::default_bindings(),
            theme: ThemeColors::default(),
            focus: Focus::Tree,
            quit: false,
            status: None,
            quit_pending: false,
        })
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Left half of the status bar: whatever needs saying, else the hint.
    pub fn status_left(&self) -> String {
        self.status.clone().unwrap_or_else(|| HINT.to_string())
    }

    /// Right half: what is open and where the cursor is, counted from 1 the way
    /// every compiler error and every other editor does.
    pub fn status_right(&self) -> String {
        let cursor = self.editor.cursor();
        format!(
            "{}  {}:{}",
            self.editor.title(),
            cursor.line + 1,
            cursor.col + 1
        )
    }

    /// Drop anything that should not outlive the next keypress.
    ///
    /// A pending quit expires with the message that announced it — otherwise a
    /// Ctrl+Q from ten minutes ago silently arms the next one.
    pub fn clear_transient(&mut self) {
        self.status = None;
        self.quit_pending = false;
    }

    /// Quit, unless a panel has something to confirm first.
    fn request_quit(&mut self) {
        if self.quit_pending {
            self.quit = true;
            return;
        }
        let blocker = self
            .editor
            .needs_close_confirmation()
            .or_else(|| self.tree.needs_close_confirmation());
        match blocker {
            Some(message) => {
                self.status = Some(format!(
                    "{message}  Ctrl+Q again to discard, Ctrl+S to save."
                ));
                self.quit_pending = true;
            }
            None => self.quit = true,
        }
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

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub fn set_keymap(&mut self, keymap: Keymap) {
        self.keymap = keymap;
    }

    /// Route one keypress.
    ///
    /// Order matters and is deliberate:
    ///
    /// 1. A bound chord becomes an `Action`, and the focused panel gets first
    ///    refusal. `None` means "I do not handle this action", which is a
    ///    different answer from handling it and having nothing to report.
    /// 2. Then the app tries it — focus, quit, save.
    /// 3. Then the panel gets the *raw key*, because a bound chord may still
    ///    mean something to a panel that has no action for it.
    /// 4. Anything unbound and printable is text. A chord carrying Ctrl or Alt
    ///    is never text — that is what stops an unbound Ctrl+J typing a `j`.
    ///
    /// Step 3 is not in the milestone plan and the file tree does not work
    /// without it. The tree navigates on raw `Up`/`Down`/`Enter`/`Left`/`Right`,
    /// and the keymap binds all five to editor actions, so a dispatcher that
    /// stops after step 2 swallows every key the tree needs.
    ///
    /// ponytail: the honest fix is naming the tree's primitives as actions the
    /// way the editor's are — "activate the selected entry" has no name today.
    /// That is a command-surface question and it lands with the palette at M4;
    /// until then the raw-key fallback is four lines and invents no vocabulary
    /// that would have to be guessed at now and lived with later.
    pub fn handle_chord(&mut self, chord: KeyChord) -> Result<()> {
        // Every key except Ctrl+Q retires the current status message and any
        // quit it left pending, so a confirmation is answered by the very next
        // keystroke or not at all.
        if chord.canonical != "ctrl+q" {
            self.clear_transient();
        }

        if let Some(action) = self.keymap.lookup(&chord) {
            if let Some(events) = self.focused_mut().apply_action(action) {
                return self.apply(events);
            }
            if self.perform_app_action(action) {
                return Ok(());
            }
            let events = self.focused_mut().handle_key(chord);
            return self.apply(events);
        }

        let is_chorded = chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if let KeyCode::Char(c) = chord.raw.code
            && !is_chorded
            && let Some(events) = self.focused_mut().apply_action(Action::InsertChar(c))
        {
            return self.apply(events);
        }

        // Unbound and not text: the panel may still want it.
        let events = self.focused_mut().handle_key(chord);
        self.apply(events)
    }

    /// Actions no panel claimed. Returns whether the app handled it.
    ///
    /// The bool is what lets an unclaimed action fall through to the raw key
    /// rather than being silently dropped — `_ => {}` here would look identical
    /// and would be the bug that kills the file tree.
    fn perform_app_action(&mut self, action: Action) -> bool {
        match action {
            Action::FocusNext => self.cycle_focus(),
            Action::Quit => self.request_quit(),
            Action::Save => match self.editor.save() {
                Ok(()) => self.status = Some("Saved.".to_string()),
                // A save that fails silently is how work gets lost.
                Err(e) => self.status = Some(format!("Save failed: {e:#}")),
            },
            _ => return false,
        }
        true
    }

    /// Process events emitted by panels.
    pub fn apply(&mut self, events: Vec<PanelEvent>) -> Result<()> {
        for event in events {
            match event {
                PanelEvent::Quit => self.request_quit(),
                PanelEvent::OpenFile { path, .. } | PanelEvent::OpenWith { path, .. } => {
                    self.open_path(&path)?;
                }
                // Redraw happens every loop pass in the walking skeleton.
                PanelEvent::NeedsRedraw => {}
                // Two fixed panels, so these are no-ops until the layout
                // system lands.
                PanelEvent::CloseSelf | PanelEvent::Focus(_) => {}
                PanelEvent::Notify { message, .. } => self.status = Some(message),
                PanelEvent::RunCommand { .. } => {}
            }
        }
        Ok(())
    }

    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (body, status_area) = crate::layout::split_frame(frame.area());
        let (tree_area, editor_area) = crate::layout::split(body);
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

        self.render_status(status_area, frame.buffer_mut());

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

    fn render_status(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let style = Style::default()
            .fg(self.theme.status_bar_fg)
            .bg(self.theme.status_bar_bg);
        let left = self.status_left();
        let right = self.status_right();

        // The right half is the fixed cost; the left is truncated to whatever
        // is left over, so a long message never pushes the position off-screen.
        let width = area.width as usize;
        let room = width.saturating_sub(right.chars().count() + 2);
        let left: String = left.chars().take(room).collect();
        let gap = width.saturating_sub(left.chars().count() + right.chars().count());
        let line = format!("{left}{}{right}", " ".repeat(gap));

        Paragraph::new(Line::raw(line))
            .style(style)
            .render(area, buf);
    }

    fn focused(&self) -> &dyn Panel {
        match self.focus {
            Focus::Tree => &self.tree,
            Focus::Editor => &self.editor,
        }
    }

    /// Areas for hit-testing mouse events, in the same order as `render`.
    /// Excludes the status bar row, so a click on it hits neither panel.
    pub fn areas(&self, area: Rect) -> (Rect, Rect) {
        let (body, _) = crate::layout::split_frame(area);
        crate::layout::split(body)
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
