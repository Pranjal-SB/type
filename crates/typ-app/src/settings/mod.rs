//! The settings tab (interface §8 4c, §8b): every row of the settings table,
//! searched, grouped under labelled rules.
//!
//! A view in the registry's sense: it opens through `OpenWith` on the
//! `config.toml` it edits, reads that file itself, and talks to the app only in
//! `PanelEvent`s (invariant 5). `layout.rs` is its one geometry, for the
//! drawing and the clicks alike; `edit.rs` changes values.

mod edit;
mod layout;

use std::any::Any;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{HandlerId, KeyChord, Panel, PanelEvent, RenderContext};

use crate::config::schema::{ROWS, Row};
use crate::config::{Settings, load_settings};
use edit::Edit;
use layout::{Item, Target};

/// The handler the settings tab registers under.
pub const SETTINGS: HandlerId = HandlerId("settings");

/// Build the tab for the `config.toml` at `path`.
pub fn open(path: &Path) -> Box<dyn Panel> {
    Box::new(SettingsView::new(path))
}

/// What the tab's own keys do. Read from [`KEYS`], a table rather than a
/// match on the chord (invariant 3), the way the tree's raw keys are the
/// tree's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    /// Between the search row and the list.
    Zone,
    Backspace,
    Escape,
    Left,
    Right,
    Enter,
    Toggle,
    Reset,
    Undo,
    OpenRaw,
}

const KEYS: &[(&str, Key)] = &[
    ("up", Key::Up),
    ("down", Key::Down),
    ("tab", Key::Zone),
    ("backtab", Key::Zone),
    ("shift+backtab", Key::Zone),
    ("backspace", Key::Backspace),
    ("esc", Key::Escape),
    ("left", Key::Left),
    ("right", Key::Right),
    ("enter", Key::Enter),
    (" ", Key::Toggle),
    ("r", Key::Reset),
    ("ctrl+z", Key::Undo),
    ("ctrl+o", Key::OpenRaw),
];

pub struct SettingsView {
    path: PathBuf,
    settings: Settings,
    /// The keys `config.toml` sets, so a row can say where its value came from.
    in_file: Vec<String>,
    query: String,
    /// The row with the keys, as an index into `ROWS`; `None` is the search
    /// row. An index into the table rather than into what is on screen, so a
    /// filter that keeps the row keeps the selection.
    selected: Option<usize>,
    /// The first line of the list on screen.
    top: usize,
    /// Where the last frame drew the tab, for the clicks that land on it.
    area: Rect,
    /// The control being worked, on the selected row.
    edit: Option<Edit>,
    /// The file before each change, newest last, and which setting it was.
    undo: Vec<(&'static str, Option<String>)>,
    /// The setting the last change was to, until the app takes it.
    changed: Option<&'static str>,
}

impl SettingsView {
    fn new(path: &Path) -> Self {
        let mut view = SettingsView {
            path: path.to_path_buf(),
            settings: Settings::default(),
            in_file: Vec::new(),
            query: String::new(),
            selected: None,
            top: 0,
            area: Rect::default(),
            edit: None,
            undo: Vec::new(),
            changed: None,
        };
        view.reload();
        view
    }

    /// Read `config.toml` again. A file that does not parse reads as the
    /// defaults, which is what the editor itself is running on.
    fn reload(&mut self) {
        self.settings = load_settings(Some(&self.path)).0;
        self.in_file = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .map(|table| table.keys().cloned().collect())
            .unwrap_or_default();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The selected setting's key, `None` while the search row has the keys.
    pub fn selected(&self) -> Option<&'static str> {
        self.selected.map(|index| ROWS[index].key)
    }

    /// The value differs from its default.
    fn modified(&self, row: &Row) -> bool {
        (row.show)(&self.settings) != row.default
    }

    /// Where the value came from: the file, or nowhere.
    fn source(&self, row: &Row) -> String {
        if self.in_file.iter().any(|key| key == row.key) {
            self.path
                .file_name()
                .map_or_else(|| "config.toml".into(), |n| n.to_string_lossy().into())
        } else {
            "built in".to_string()
        }
    }

    /// The rows the query keeps, as indices into `ROWS`, in table order.
    ///
    /// `@section` keeps a section by the start of its slug, `:modified` keeps
    /// changed values, and any other word has to appear in the key, the
    /// description or the section.
    fn visible(&self) -> Vec<usize> {
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        (0..ROWS.len())
            .filter(|&index| {
                let row = &ROWS[index];
                words.iter().all(|word| {
                    if let Some(section) = word.strip_prefix('@') {
                        slug(row.section).starts_with(section)
                    } else if word == ":modified" {
                        self.modified(row)
                    } else {
                        row.key.contains(word.as_str())
                            || row.description.contains(word.as_str())
                            || row.section.contains(word.as_str())
                    }
                })
            })
            .collect()
    }

    fn step(&mut self, down: bool) {
        self.edit = None;
        let visible = self.visible();
        let at = self
            .selected
            .and_then(|index| visible.iter().position(|&v| v == index));
        self.selected = match (at, down) {
            (None, true) => visible.first().copied(),
            (None, false) => None,
            (Some(at), true) => Some(visible.get(at + 1).copied().unwrap_or(visible[at])),
            (Some(0), false) => None,
            (Some(at), false) => Some(visible[at - 1]),
        };
    }

    fn type_char(&mut self, c: char) {
        self.edit = None;
        self.query.push(c);
        self.selected = None;
        self.top = 0;
    }

    fn key(&mut self, key: Key) -> Vec<PanelEvent> {
        let redraw = vec![PanelEvent::NeedsRedraw];
        match (key, self.selected) {
            (Key::Up, _) => self.step(false),
            (Key::Down, _) => self.step(true),
            (Key::Zone, Some(_)) => self.selected = None,
            (Key::Zone, None) => self.selected = self.visible().first().copied(),
            (Key::Backspace, _) => {
                self.query.pop();
                self.selected = None;
                self.top = 0;
            }
            (Key::Escape, Some(_)) => self.selected = None,
            (Key::Escape, None) if !self.query.is_empty() => {
                self.query.clear();
                self.top = 0;
            }
            (Key::Escape, None) => return Vec::new(),
            (Key::Left, Some(index)) => return self.cycle(index, -1),
            (Key::Right, Some(index)) => return self.cycle(index, 1),
            (Key::Enter, Some(index)) => return self.activate(index),
            (Key::Enter, None) => self.selected = self.visible().first().copied(),
            (Key::Toggle, Some(index)) => return self.toggle(index),
            (Key::Reset, Some(index)) => return self.reset(index),
            (Key::Undo, _) => return self.undo(),
            (Key::OpenRaw, _) => return self.open_raw(),
            (Key::Left | Key::Right | Key::Toggle | Key::Reset, None) => return Vec::new(),
        }
        redraw
    }

    fn click(&mut self, column: u16, row: u16) -> Vec<PanelEvent> {
        let geometry = self.geometry(self.area);
        let at = (column, row).into();
        let redraw = vec![PanelEvent::NeedsRedraw];
        if let Some((_, target)) = geometry.targets.iter().find(|(rect, _)| rect.contains(at)) {
            return match *target {
                Target::Less(index) => {
                    self.selected = Some(index);
                    self.cycle(index, -1)
                }
                Target::More(index) => {
                    self.selected = Some(index);
                    self.cycle(index, 1)
                }
                Target::Value(index) => self.activate(index),
                Target::Choice(index, n) => self.pick(index, n),
                Target::Reset => match self.selected {
                    Some(index) => self.reset(index),
                    None => vec![PanelEvent::Notify {
                        level: typ_core::NotifyLevel::Info,
                        message: "Select a setting to reset.".to_string(),
                    }],
                },
                Target::Undo => self.undo(),
                Target::OpenRaw => self.open_raw(),
            };
        }
        // Anywhere else puts a control being worked away, as `esc` would.
        self.edit = None;
        if geometry.search.contains(at) {
            self.selected = None;
            return redraw;
        }
        if let Some((_, section)) = geometry.sections.iter().find(|(rect, _)| rect.contains(at)) {
            self.query = format!("@{}", slug(section));
            self.selected = None;
            self.top = 0;
            return redraw;
        }
        if let Some((_, item)) = geometry.items.iter().find(|(rect, _)| rect.contains(at)) {
            match item {
                Item::Name(index) | Item::Detail(index) => self.selected = Some(*index),
                Item::Rule(_) | Item::Choice(..) => {}
            }
        }
        redraw
    }
}

/// A section as a query names it: `editor › indentation` is
/// `editor/indentation`, so `@editor` keeps both editor sections.
fn slug(section: &str) -> String {
    section.replace(" \u{203a} ", "/").replace(' ', "-")
}

impl Panel for SettingsView {
    fn name(&self) -> &'static str {
        "settings"
    }

    fn title(&self) -> String {
        "Settings".to_string()
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        self.area = area;
        self.draw(area, buf, ctx);
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        if let Some(events) = self.edit_key(&chord) {
            return events;
        }
        let plain = match chord.raw.code {
            KeyCode::Char(c)
                if !chord
                    .raw
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Some(c)
            }
            _ => None,
        };
        let key = KEYS
            .iter()
            .find(|(name, _)| *name == chord.canonical)
            .map(|(_, key)| *key);
        match (key, plain) {
            // On the search row a letter is the query, `r` and space too.
            (Some(Key::Toggle | Key::Reset), Some(c)) if self.selected.is_none() => {
                self.type_char(c);
                vec![PanelEvent::NeedsRedraw]
            }
            (Some(key), _) => self.key(key),
            (None, Some(c)) => {
                self.type_char(c);
                vec![PanelEvent::NeedsRedraw]
            }
            (None, None) => Vec::new(),
        }
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        self.area = panel_area;
        self.click(event.column, event.row)
    }

    fn handle_scroll(&mut self, delta: i32, _panel_area: Rect) -> Vec<PanelEvent> {
        self.top = self.top.saturating_add_signed(delta as isize);
        vec![PanelEvent::NeedsRedraw]
    }

    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        if self.selected.is_some() {
            return None;
        }
        let search = self.geometry(panel_area).search;
        let x = search.x
            + layout::SEARCH_LABEL.len() as u16
            + typ_buffer::display_width(&self.query) as u16;
        Some((x.min(search.right().saturating_sub(1)), search.y))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
