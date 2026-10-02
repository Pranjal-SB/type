//! Dispatch: a key, a paste or a named action, and where each one goes.
//!
//! A child module of `app` for the reason `search` is. Split out at M4 with
//! `servers` and `background`: routing is its own responsibility, and the one
//! the layout tree changes most, since "the focused panel" stops being one of
//! two fixed things.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use typ_core::{Action, Direction, KeyChord, PanelEvent};

use super::App;
use crate::prompt::{Prompt, PromptKind};

/// The two actions whose confirmation is the next press of the same thing.
///
/// Both dispatch paths have to make the same exception, because clearing
/// transient state on the way in erases the answer these two are about to
/// read. It is a function rather than a `matches!` in each of them because it
/// was already wrong in one: the palette cleared the flag it then set, so on a
/// dirty buffer `quit` and `close_tab` could never complete from it: the
/// message just came back forever. Gap 68.
fn answers_its_own_confirmation(action: Action) -> bool {
    matches!(action, Action::Quit | Action::CloseTab)
}

/// The prefix `Action::OpenMenu` opens, when no key was pressed to say which.
const DOOR: &str = "ctrl+k";

impl App {
    /// Run an action by name, the way the command palette does.
    ///
    /// Panel first, then the app: the same order `handle_chord` uses, because
    /// an action reachable one way and not the other is exactly the split the
    /// palette exists to close.
    pub fn apply_named_action(&mut self, action: Action) -> Result<()> {
        if !answers_its_own_confirmation(action) {
            self.clear_transient();
        }
        if let Some(events) = self.focused_mut().apply_action(action) {
            return self.apply(events);
        }
        self.perform_app_action(action);
        Ok(())
    }

    /// Route one keypress.
    ///
    /// Order matters and is deliberate:
    ///
    /// 1. A bound chord becomes an `Action`, and the focused panel gets first
    ///    refusal. `None` means "I do not handle this action", which is a
    ///    different answer from handling it and having nothing to report.
    /// 2. Then the app tries it: focus, quit, save.
    /// 3. Then the panel gets the *raw key*, because a bound chord may still
    ///    mean something to a panel that has no action for it.
    /// 4. Anything unbound and printable is text. A chord carrying Ctrl or Alt
    ///    is never text; that is what stops an unbound Ctrl+J typing a `j`.
    ///
    /// Step 3 is not in the milestone plan and the file tree does not work
    /// without it. The tree navigates on raw `Up`/`Down`/`Enter`/`Left`/`Right`,
    /// and the keymap binds all five to editor actions, so a dispatcher that
    /// stops after step 2 swallows every key the tree needs.
    ///
    /// ponytail: the honest fix is naming the tree's primitives as actions the
    /// way the editor's are: "activate the selected entry" has no name today.
    ///
    /// **This comment used to say that lands with the palette. The palette
    /// landed and it did not.** The two turned out to be independent: the
    /// palette lists whatever is in `Action::ALL`, and the tree's primitives
    /// are not in it, so the palette covers the editor and the app and the
    /// raw-key fallback stays exactly as it was. Naming five tree primitives is
    /// a vocabulary decision with no second consumer asking for it yet, which
    /// is the only thing invariant 2 actually requires. It moves to the
    /// milestone that gives the tree a keymap of its own.
    pub fn handle_chord(&mut self, chord: KeyChord) -> Result<()> {
        // The picker owns the keyboard while it is up, ahead of the prompt and
        // ahead of the keymap, otherwise typing a filename fires every editing
        // action bound to a letter, which edits the buffer behind the overlay.
        if self.picker.is_some() {
            return self.handle_picker_chord(chord);
        }

        // An open prompt owns the keyboard, ahead of everything. Routing
        // through the keymap first would let a chord bound to an editing action
        // fire while the user is typing a search term.
        if self.prompt.is_some() {
            return self.handle_prompt_chord(chord);
        }

        if self.menu.is_some() {
            return self.handle_menu_chord(chord);
        }

        let bound = match self.keymap.resolve(None, &chord) {
            typ_core::Resolved::Matched(action) => Some(action),
            // Nothing runs yet, so nothing transient is cleared: a close
            // armed by `ctrl+k w` has to survive the `ctrl+k` that confirms it.
            typ_core::Resolved::Pending(rows) => {
                self.menu = Some(crate::menu::Menu::new(chord.canonical, rows));
                return Ok(());
            }
            typ_core::Resolved::NotFound => None,
        };

        // Every key retires the current status message and anything it left
        // pending, so a confirmation is answered by the very next keystroke or
        // not at all, except the keys whose confirmation it is. Those two ask
        // "press it again", so clearing on the way in would erase the answer
        // they are about to read.
        //
        // Keyed on the action rather than on the chord: this used to compare
        // `chord.canonical` against the literal `"ctrl+q"`, which meant
        // rebinding quit silently broke its own confirmation.
        if !bound.is_some_and(answers_its_own_confirmation) {
            self.clear_transient();
        }

        if let Some(action) = bound {
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

    /// Text arriving as one bracketed-paste event rather than as N keypresses.
    ///
    /// Without this a paste is delivered a character at a time: one loop pass
    /// and one repaint each, and, worse, any chord inside the pasted text
    /// executes as a command instead of being inserted.
    ///
    /// The text travels through the clipboard register rather than through an
    /// `Action` carrying a `String`. `Action` is `Copy` and the keymap depends
    /// on that; widening it for one variant would touch the keymap, its tests
    /// and every match in the dispatcher. Pasting is still an action, still
    /// reachable from the palette and the vim layer; the payload just goes the
    /// way payloads already go.
    pub fn handle_paste(&mut self, text: String) -> Result<()> {
        self.clear_transient();

        // Same order as `handle_chord`: the overlay is ahead of the prompt.
        if self.picker.is_some() {
            self.paste_into_picker(&text);
            return Ok(());
        }

        // A paste into an open prompt is a search term, not an edit.
        if let Some(prompt) = self.prompt.as_mut() {
            for ch in text.chars().filter(|c| !c.is_control()) {
                prompt.insert_char(ch);
            }
            return Ok(());
        }

        typ_buffer::clipboard::set_register(&text);
        let events = self
            .focused_mut()
            .apply_action(Action::Paste)
            .unwrap_or_default();
        self.apply(events)
    }

    /// Actions no panel claimed. Returns whether the app handled it.
    ///
    /// The bool is what lets an unclaimed action fall through to the raw key
    /// rather than being silently dropped: `_ => {}` here would look identical
    /// and would be the bug that kills the file tree.
    pub(super) fn perform_app_action(&mut self, action: Action) -> bool {
        match action {
            Action::FocusNext => self.cycle_focus(),
            Action::FocusPrevious => self.step_focus(-1),
            // Only reached when the focused panel declined it, which the
            // editor never does: a selection to collapse is its own thing to
            // cancel, and it gets that first.
            Action::CollapseSelections => return self.focus_back(),
            Action::OpenFilePicker => self.open_picker(),
            Action::OpenProjectSearch => self.open_search(),
            Action::OpenCommandPalette => self.open_command_palette(),
            Action::OpenMenu => self.open_menu(DOOR),
            Action::Quit => self.request_quit(),
            Action::NextTab => self.next_tab(),
            Action::PrevTab => self.prev_tab(),
            Action::CloseTab => self.request_close_tab(self.active),
            // Counted from one, and a digit past the last open file is a
            // no-op rather than a clamp: landing on the last tab because you
            // pressed Alt+9 with three open is a jump you did not ask for.
            Action::GoToTab(n) => self.activate_tab((n as usize).saturating_sub(1)),
            Action::RestartLanguageServers => {
                let started = self.lsp.restart_all();
                self.status = Some(match started {
                    0 => "No language server to restart.".to_string(),
                    1 => "Restarted the language server.".to_string(),
                    n => format!("Restarted {n} language servers."),
                });
            }
            Action::GotoDefinition => self.ask_server(crate::lsp::Ask::Definition),
            Action::Hover => self.ask_server(crate::lsp::Ask::Hover),
            Action::Save => match self.tabs[self.active].panel.save() {
                Ok(()) => {
                    self.status = Some("Saved.".to_string());
                    // After the atomic save, never before it: a server told
                    // about a write that then failed is holding a document
                    // TYPE does not have.
                    self.notify_saved();
                }
                // A save that fails silently is how work gets lost. The status
                // bar says so and the log keeps the whole error chain, which is
                // the part that is actually diagnosable.
                Err(e) => {
                    crate::log_error!("save failed: {e:#}");
                    self.status = Some(format!("Save failed: {e:#}"));
                }
            },
            Action::GotoLine => self.prompt = Some(Prompt::new(PromptKind::GotoLine)),
            Action::SearchOpen => self.prompt = Some(Prompt::new(PromptKind::Search)),
            Action::ReplaceOpen => {
                let mut prompt = Prompt::new(PromptKind::Search);
                prompt.become_replace_after_needle();
                self.prompt = Some(prompt);
            }
            Action::SearchNext | Action::SearchPrevious => {
                let Some(query) = self.last_query.clone() else {
                    self.status = Some("Nothing to search for yet".to_string());
                    return true;
                };
                let direction = if action == Action::SearchNext {
                    Direction::Forward
                } else {
                    Direction::Backward
                };
                self.jump_to_match(&query, direction);
            }
            _ => return false,
        }
        true
    }

    /// Process events emitted by panels.
    pub fn apply(&mut self, events: Vec<PanelEvent>) -> Result<()> {
        for event in events {
            match event {
                PanelEvent::OpenFile { path, at } => {
                    let (line, col) = (at.line, at.col);
                    // **The event has carried `line` and `col` since M1 and
                    // nothing read them until M2.8.** Harmless while the only
                    // producer was the file tree, which always means the top of
                    // the file; a project-search result that opens at line 0 has
                    // thrown away the only thing the search found out.
                    if self.open_or_report(&path) && (line > 0 || col > 0) {
                        self.tabs[self.active].panel.goto(line, col);
                    }
                }
                PanelEvent::OpenWith { path, .. } => {
                    self.open_or_report(&path);
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
}
