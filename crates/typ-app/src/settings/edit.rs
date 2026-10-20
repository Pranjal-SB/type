//! Changing a value: the controls, the write, undo, and what the app is told.
//!
//! Every change goes through `config::write_setting`, so `config.toml` keeps
//! its comments, and leaves the old file on an undo stack. The tab then reads
//! the file back, so what it shows is what the file says, not what it meant
//! to write.

use crossterm::event::{KeyCode, KeyModifiers};
use typ_core::{KeyChord, NotifyLevel, PanelEvent, Position};

use super::SettingsView;
use crate::config::schema::{Kind, ROWS, Row};
use crate::config::{Settings, restore_config, write_setting};

/// A control being worked: the list under an enum, or a number field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Edit {
    /// The choices of the selected row, `at` the highlighted one.
    List { at: usize },
    /// The number field's text, and why the last answer was refused.
    Field {
        input: String,
        error: Option<String>,
    },
}

impl SettingsView {
    /// The names a choice row offers. A theme's are the shipped ones, then
    /// any in the themes directory beside `config.toml`.
    pub(super) fn choices(&self, row: &Row) -> Vec<String> {
        match row.kind {
            Kind::Choice(choices) => choices.iter().map(|c| c.to_string()).collect(),
            Kind::Theme => {
                let mut names: Vec<String> = crate::config::embedded_names()
                    .map(str::to_string)
                    .collect();
                let dir = self.path.parent().map(|dir| dir.join("themes"));
                let mut yours: Vec<String> = dir
                    .and_then(|dir| std::fs::read_dir(dir).ok())
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.ok()?.path();
                        (path.extension()? == "toml")
                            .then(|| path.file_stem()?.to_str().map(str::to_string))?
                    })
                    .filter(|name| !names.contains(name))
                    .collect();
                yours.sort();
                names.extend(yours);
                names
            }
            _ => Vec::new(),
        }
    }

    /// The next or previous choice, written.
    pub(super) fn cycle(&mut self, index: usize, step: isize) -> Vec<PanelEvent> {
        let row = &ROWS[index];
        let choices = self.choices(row);
        if choices.is_empty() {
            return Vec::new();
        }
        let current = (row.show)(&self.settings);
        let at = choices.iter().position(|c| *c == current).unwrap_or(0) as isize;
        let next = (at + step).rem_euclid(choices.len() as isize) as usize;
        let value = choice_value(&choices[next]);
        self.set(index, value)
    }

    /// `enter`, or a click on the value: open the list, flip the box, edit
    /// the number, or open the file for a setting the tab does not edit.
    pub(super) fn activate(&mut self, index: usize) -> Vec<PanelEvent> {
        self.selected = Some(index);
        let row = &ROWS[index];
        match row.kind {
            Kind::Choice(_) | Kind::Theme => {
                let current = (row.show)(&self.settings);
                let at = self
                    .choices(row)
                    .iter()
                    .position(|c| *c == current)
                    .unwrap_or(0);
                self.edit = Some(Edit::List { at });
                vec![PanelEvent::NeedsRedraw]
            }
            Kind::Toggle => self.toggle(index),
            Kind::Number { .. } => {
                let current = (row.show)(&self.settings);
                let input = if current == "auto" {
                    String::new()
                } else {
                    current
                };
                self.edit = Some(Edit::Field { input, error: None });
                vec![PanelEvent::NeedsRedraw]
            }
            Kind::File => self.open_raw(),
        }
    }

    pub(super) fn toggle(&mut self, index: usize) -> Vec<PanelEvent> {
        let row = &ROWS[index];
        if row.kind != Kind::Toggle {
            return Vec::new();
        }
        let on = (row.show)(&self.settings) == "true";
        self.set(index, Some((!on).into()))
    }

    /// `r`: the default back, by taking the key out of the file.
    pub(super) fn reset(&mut self, index: usize) -> Vec<PanelEvent> {
        self.edit = None;
        self.set(index, None)
    }

    pub(super) fn open_raw(&self) -> Vec<PanelEvent> {
        vec![PanelEvent::OpenFile {
            path: self.path.clone(),
            at: Position::default(),
        }]
    }

    /// Write `value` for row `index`, `None` to remove the key.
    fn set(&mut self, index: usize, value: Option<toml_edit::Value>) -> Vec<PanelEvent> {
        let row = &ROWS[index];
        let before = std::fs::read_to_string(&self.path).ok();
        if let Err(e) = write_setting(&self.path, row.key, value) {
            return vec![PanelEvent::Notify {
                level: NotifyLevel::Error,
                message: format!("Could not write {}: {e}", row.key),
            }];
        }
        self.undo.push((row.key, before));
        self.reload();
        self.changed = Some(row.key);
        self.applied(row)
    }

    /// `ctrl+z`: the file as it was before the last change.
    pub(super) fn undo(&mut self) -> Vec<PanelEvent> {
        let Some((key, before)) = self.undo.pop() else {
            return vec![PanelEvent::Notify {
                level: NotifyLevel::Info,
                message: "Nothing to undo in the settings.".to_string(),
            }];
        };
        if let Err(e) = restore_config(&self.path, before.as_deref()) {
            self.undo.push((key, before));
            return vec![PanelEvent::Notify {
                level: NotifyLevel::Error,
                message: format!("Could not undo: {e}"),
            }];
        }
        self.edit = None;
        self.reload();
        self.changed = Some(key);
        match ROWS.iter().find(|row| row.key == key) {
            Some(row) => self.applied(row),
            None => vec![PanelEvent::NeedsRedraw],
        }
    }

    /// What a change says: nothing but a repaint, unless it waits for the
    /// next start.
    fn applied(&self, row: &Row) -> Vec<PanelEvent> {
        let mut events = vec![PanelEvent::NeedsRedraw];
        if row.restart {
            events.push(PanelEvent::Notify {
                level: NotifyLevel::Info,
                message: format!("{} takes effect on the next start.", row.key),
            });
        }
        events
    }

    /// The last change, once: which setting, and everything as the file now
    /// says. The app applies it live where it can.
    pub fn take_changed(&mut self) -> Option<(&'static str, Settings)> {
        self.changed.take().map(|key| (key, self.settings.clone()))
    }

    /// A key while a control is being worked. `None` when nothing is.
    pub(super) fn edit_key(&mut self, chord: &KeyChord) -> Option<Vec<PanelEvent>> {
        let index = self.selected?;
        let edit = self.edit.clone()?;
        let row = &ROWS[index];
        let redraw = vec![PanelEvent::NeedsRedraw];
        Some(match edit {
            Edit::List { at } => {
                let count = self.choices(row).len();
                match chord.canonical.as_str() {
                    "up" => {
                        self.edit = Some(Edit::List {
                            at: at.saturating_sub(1),
                        });
                        redraw
                    }
                    "down" => {
                        self.edit = Some(Edit::List {
                            at: (at + 1).min(count.saturating_sub(1)),
                        });
                        redraw
                    }
                    "enter" => self.pick(index, at),
                    "esc" => {
                        self.edit = None;
                        redraw
                    }
                    _ => redraw,
                }
            }
            Edit::Field { mut input, .. } => match chord.raw.code {
                KeyCode::Enter => self.commit(index, input),
                KeyCode::Esc => {
                    self.edit = None;
                    redraw
                }
                KeyCode::Backspace => {
                    input.pop();
                    self.edit = Some(Edit::Field { input, error: None });
                    redraw
                }
                KeyCode::Char(c)
                    if !chord
                        .raw
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    input.push(c);
                    self.edit = Some(Edit::Field { input, error: None });
                    redraw
                }
                _ => redraw,
            },
        })
    }

    /// Choice `at` of row `index`, written, and the list put away.
    pub(super) fn pick(&mut self, index: usize, at: usize) -> Vec<PanelEvent> {
        self.edit = None;
        let choices = self.choices(&ROWS[index]);
        match choices.get(at) {
            Some(choice) => self.set(index, choice_value(choice)),
            None => vec![PanelEvent::NeedsRedraw],
        }
    }

    /// `enter` in the number field: write it, or say why not and write
    /// nothing. Empty is `auto`.
    fn commit(&mut self, index: usize, input: String) -> Vec<PanelEvent> {
        let row = &ROWS[index];
        let Kind::Number { min, max } = row.kind else {
            self.edit = None;
            return Vec::new();
        };
        let text = input.trim();
        if text.is_empty() {
            self.edit = None;
            return self.set(index, None);
        }
        match text.parse::<i64>().ok().filter(|n| (min..=max).contains(n)) {
            Some(n) => {
                self.edit = None;
                self.set(index, Some(n.into()))
            }
            None => {
                self.edit = Some(Edit::Field {
                    input,
                    error: Some(format!(
                        "{} takes a whole number from {min} to {max}, or nothing for auto",
                        row.key
                    )),
                });
                vec![PanelEvent::NeedsRedraw]
            }
        }
    }
}

/// A choice as `config.toml` holds it: `auto` is the key left out.
fn choice_value(choice: &str) -> Option<toml_edit::Value> {
    (choice != "auto").then(|| choice.into())
}
