//! The `ctrl+k` door: a prefix pressed, the menu it puts up, and the key or
//! click that follows.
//!
//! A child module of `app` for the reason `picker` is: it reaches `App`'s
//! private fields without widening them.

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_core::{Action, KeyChord, Resolved};

use super::App;
use crate::menu::Menu;

/// How long the status bar teaches the chord a menu row stood for.
const TEACH_FOR: Duration = Duration::from_secs(2);

impl App {
    /// The first key of a sequence, if one is waiting for its second.
    pub fn pending_prefix(&self) -> Option<&str> {
        self.menu.as_ref().map(Menu::prefix)
    }

    /// Put the menu up for `prefix`, as pressing it would.
    pub(super) fn open_menu(&mut self, prefix: &str) {
        let rows = self.keymap.under(prefix);
        if rows.is_empty() {
            self.status = Some(format!("Nothing is bound under {prefix}."));
            return;
        }
        self.menu = Some(Menu::new(prefix.to_string(), rows));
    }

    /// The key after a prefix.
    ///
    /// A key bound under the prefix runs, whatever else it is, so a rebind to
    /// `ctrl+k enter` beats the menu's own use of Enter. Then the menu's keys:
    /// arrows move, Enter runs the highlight, Esc cancels. Anything else ends
    /// the sequence and says it was not bound.
    pub(super) fn handle_menu_chord(&mut self, chord: KeyChord) -> Result<()> {
        let Some(mut menu) = self.menu.take() else {
            return Ok(());
        };
        if chord.raw.code == KeyCode::Esc {
            return Ok(());
        }
        if let Resolved::Matched(action) = self.keymap.resolve(Some(menu.prefix()), &chord) {
            return self.run_from_menu(menu.prefix(), &chord.canonical, action);
        }
        match chord.raw.code {
            KeyCode::Up => menu.move_selection(-1),
            KeyCode::Down => menu.move_selection(1),
            KeyCode::Enter => {
                let Some((key, action)) = menu.selected() else {
                    return Ok(());
                };
                return self.run_from_menu(menu.prefix(), key, *action);
            }
            _ => {
                self.clear_transient();
                self.status = Some(format!(
                    "{} {} is not bound",
                    menu.prefix(),
                    chord.canonical
                ));
                return Ok(());
            }
        }
        self.menu = Some(menu);
        Ok(())
    }

    /// A press while the menu is up: a row runs, anywhere else cancels.
    pub fn route_menu_mouse(&mut self, event: MouseEvent, frame: Rect) -> Result<()> {
        if !matches!(event.kind, MouseEventKind::Down(_)) {
            return Ok(());
        }
        let area = self.split_body(frame).1;
        let Some(menu) = self.menu.take() else {
            return Ok(());
        };
        let hit = area
            .and_then(|area| menu.hit(area, event.column, event.row))
            .and_then(|index| menu.row(index));
        match (event.kind, hit) {
            (MouseEventKind::Down(MouseButton::Left), Some((key, action))) => {
                self.run_from_menu(menu.prefix(), key, *action)
            }
            _ => Ok(()),
        }
    }

    /// Run a row, then teach its chord.
    ///
    /// The named-action path: panel first, then the app, and the same
    /// exemption for the two actions that confirm themselves. The teaching
    /// line is what makes the menu a way to learn the direct chord rather than
    /// a place to live (interface §6).
    fn run_from_menu(&mut self, prefix: &str, key: &str, action: Action) -> Result<()> {
        let line = format!("{prefix} {key} \u{b7} {}", action.description());
        self.apply_named_action(action)?;
        self.teaching = Some((line, Instant::now() + TEACH_FOR));
        Ok(())
    }

    /// Retire whatever has outlived its deadline.
    ///
    /// Called with each event rather than from a timer, so a line left
    /// standing costs nothing while the editor is idle and is gone the moment
    /// anything happens after its two seconds.
    /// Returns whether anything went, so the screen can be told.
    pub fn expire(&mut self, now: Instant) -> bool {
        let over = self
            .teaching
            .as_ref()
            .is_some_and(|(_, until)| now >= *until);
        if over {
            self.teaching = None;
        }
        over
    }

    /// When the loop has to wake with nothing arriving, if ever.
    ///
    /// **`None`, and the teaching line is the reason it is worth a method.**
    /// The line could ask to be cleared on time, and that would be a wakeup
    /// every time the menu is used, on an editor nobody is looking at. It
    /// expires on the next event instead; see `expire`.
    pub fn wake_deadline(&self) -> Option<Instant> {
        None
    }

    /// The body above the status bar, less the menu when it is up, and the
    /// menu's rect. Render and hit-testing both come through here.
    pub(super) fn split_body(&self, frame: Rect) -> (Rect, Option<Rect>) {
        let (body, _) = crate::layout::split_frame(frame);
        let Some(menu) = &self.menu else {
            return (body, None);
        };
        let height = menu.height(frame.width, frame.height).min(body.height);
        let (body, menu) = crate::layout::split_menu(body, height);
        (body, Some(menu))
    }
}
