//! The keybindings tab (interface §8 4d, §8b): every binding in the keymap,
//! and every action with none, searched and grouped under labelled rules.
//!
//! A view in the registry's sense, as the settings tab is: it opens through
//! `OpenWith` on the `keys.toml` it reads, builds the keymap from it the way
//! `main` does, and talks to the app only in `PanelEvent`s (invariant 5).
//! `layout.rs` is its one geometry, for the drawing and the clicks alike.
//!
//! One layer exists, global, so the layer column says so on every row and a
//! conflict is a user row that took a chord the defaults gave to something
//! else (`controls.md` §3's panel layers are not built).

mod layout;

use std::any::Any;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Action, Group, HandlerId, KeyChord, Keymap, Panel, PanelEvent, RenderContext};

use crate::config::load_keymap;
use layout::{Item, Target};

/// The handler the keybindings tab registers under.
pub const KEYBINDINGS: HandlerId = HandlerId("keybindings");

/// Build the tab for the `keys.toml` at `path`.
pub fn open(path: &Path) -> Box<dyn Panel> {
    Box::new(KeysView::new(path))
}

/// One row: a chord and what it runs, or an action with no chord.
pub type Binding = (Option<String>, Action);

/// What the tab's own keys do, read from [`KEYS`] (invariant 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    /// Between the search row and the list.
    Zone,
    Backspace,
    Escape,
    Enter,
    /// `ctrl+r`: the next key pressed is the query.
    Capture,
}

const KEYS: &[(&str, Key)] = &[
    ("up", Key::Up),
    ("down", Key::Down),
    ("tab", Key::Zone),
    ("backtab", Key::Zone),
    ("shift+backtab", Key::Zone),
    ("backspace", Key::Backspace),
    ("esc", Key::Escape),
    ("enter", Key::Enter),
    ("ctrl+r", Key::Capture),
];

pub struct KeysView {
    path: PathBuf,
    /// The defaults with `keys.toml` over them, as `main` builds it.
    keymap: Keymap,
    defaults: Keymap,
    /// The chords `keys.toml` names, canonical, so a row can say it is yours.
    user: Vec<String>,
    query: String,
    /// The row with the keys; `None` is the search row. The binding itself
    /// rather than a position, so a filter that keeps the row keeps it.
    selected: Option<Binding>,
    /// The first line of the list on screen.
    top: usize,
    /// Where the last frame drew the tab, for the clicks that land on it.
    area: Rect,
    /// `ctrl+r` was pressed: the next key is searched for, not run.
    capturing: bool,
}

impl KeysView {
    fn new(path: &Path) -> Self {
        let mut view = KeysView {
            path: path.to_path_buf(),
            keymap: Keymap::default_bindings(),
            defaults: Keymap::default_bindings(),
            user: Vec::new(),
            query: String::new(),
            selected: None,
            top: 0,
            area: Rect::default(),
            capturing: false,
        };
        view.reload();
        view
    }

    /// Read `keys.toml` again. A file that does not load reads as the
    /// defaults, which is what the editor itself falls back to.
    fn reload(&mut self) {
        self.keymap = load_keymap(Some(&self.path)).0;
        self.user = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .map(|table| {
                table
                    .keys()
                    .filter_map(|key| typ_core::key::canonical_sequence(key).ok())
                    .collect()
            })
            .unwrap_or_default();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The selected row, `None` while the search row has the keys.
    pub fn selected(&self) -> Option<Binding> {
        self.selected.clone()
    }

    /// Every row, by group in the menu's order, then by action, then by chord.
    fn rows(&self) -> Vec<Binding> {
        let mut out = Vec::new();
        for group in Group::ALL {
            for &action in Action::ALL.iter().filter(|a| a.group() == *group) {
                let chords = self.keymap.bindings_for(action);
                if chords.is_empty() {
                    out.push((None, action));
                }
                out.extend(chords.into_iter().map(|c| (Some(c.to_string()), action)));
            }
        }
        out
    }

    /// The row differs from the defaults: `keys.toml` set its chord, or took
    /// away the one the action had.
    fn modified(&self, (chord, action): &Binding) -> bool {
        match chord {
            Some(chord) => self.user.contains(chord),
            None => !self.defaults.bindings_for(*action).is_empty(),
        }
    }

    /// A user row on a chord the defaults gave to another action, which that
    /// action has lost.
    fn conflict(&self, (chord, action): &Binding) -> bool {
        chord.as_deref().is_some_and(|chord| {
            self.user.iter().any(|c| c == chord)
                && self
                    .defaults
                    .action_for(chord)
                    .is_some_and(|was| was != *action)
        })
    }

    /// The chord needs the kitty keyboard protocol to arrive at all.
    fn kitty((chord, _): &Binding) -> bool {
        chord
            .as_deref()
            .is_some_and(crate::capability::needs_protocol)
    }

    /// Where the row's chord came from.
    fn source(&self, binding: &Binding) -> &'static str {
        if self.modified(binding) {
            "user"
        } else {
            "default"
        }
    }

    /// The rows the query keeps, in row order.
    ///
    /// `:modified :conflict :unbound :kitty` are filters, `=chord` is what
    /// `ctrl+r` writes (that chord, or everything under it when it is a
    /// prefix), and the rest of the query is one phrase that has to appear in
    /// the description, the action's name or the chord.
    pub fn visible(&self) -> Vec<Binding> {
        let mut filters = Vec::new();
        let mut phrase = Vec::new();
        for word in self.query.split_whitespace() {
            if word.starts_with(':') || word.starts_with('=') {
                filters.push(word.to_lowercase());
            } else {
                phrase.push(word.to_lowercase());
            }
        }
        let phrase = phrase.join(" ");
        self.rows()
            .into_iter()
            .filter(|row| filters.iter().all(|f| self.passes(f, row)))
            .filter(|(chord, action)| {
                phrase.is_empty()
                    || action.description().contains(&phrase)
                    || action.name().contains(&phrase)
                    || chord.as_deref().is_some_and(|c| c.contains(&phrase))
            })
            .collect()
    }

    fn passes(&self, filter: &str, row: &Binding) -> bool {
        if let Some(pressed) = filter.strip_prefix('=') {
            return row.0.as_deref().is_some_and(|chord| {
                chord == pressed
                    || chord
                        .strip_prefix(pressed)
                        .is_some_and(|rest| rest.starts_with(' '))
            });
        }
        match filter {
            ":modified" => self.modified(row),
            ":conflict" => self.conflict(row),
            ":unbound" => row.0.is_none(),
            ":kitty" => Self::kitty(row),
            // A filter still being typed keeps everything.
            _ => true,
        }
    }

    fn step(&mut self, down: bool) {
        let visible = self.visible();
        let at = self
            .selected
            .as_ref()
            .and_then(|row| visible.iter().position(|v| v == row));
        self.selected = match (at, down) {
            (None, true) => visible.first().cloned(),
            (None, false) => None,
            (Some(at), true) => visible.get(at + 1).or(visible.get(at)).cloned(),
            (Some(0), false) => None,
            (Some(at), false) => visible.get(at - 1).cloned(),
        };
    }

    fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.selected = None;
        self.top = 0;
    }

    /// The key `ctrl+r` was waiting for, as the query.
    fn captured(&mut self, chord: &KeyChord) {
        self.capturing = false;
        self.query = format!("={}", chord.canonical);
        self.selected = None;
        self.top = 0;
    }

    fn key(&mut self, key: Key) -> Vec<PanelEvent> {
        match key {
            Key::Up => self.step(false),
            Key::Down => self.step(true),
            Key::Zone if self.selected.is_some() => self.selected = None,
            Key::Zone | Key::Enter => self.selected = self.visible().first().cloned(),
            Key::Backspace => {
                self.query.pop();
                self.selected = None;
                self.top = 0;
            }
            Key::Escape if self.selected.is_some() => self.selected = None,
            Key::Escape if !self.query.is_empty() => {
                self.query.clear();
                self.top = 0;
            }
            Key::Escape => return Vec::new(),
            Key::Capture => self.capturing = true,
        }
        vec![PanelEvent::NeedsRedraw]
    }

    fn click(&mut self, column: u16, row: u16) -> Vec<PanelEvent> {
        let geometry = self.geometry(self.area);
        let at = (column, row).into();
        if let Some((_, target)) = geometry.targets.iter().find(|(rect, _)| rect.contains(at)) {
            return match target {
                Target::Capture => self.key(Key::Capture),
            };
        }
        if geometry.search.contains(at) {
            self.selected = None;
        } else if let Some((_, Item::Row(binding))) =
            geometry.items.iter().find(|(rect, _)| rect.contains(at))
        {
            self.selected = Some(binding.clone());
        }
        vec![PanelEvent::NeedsRedraw]
    }
}

impl Panel for KeysView {
    fn name(&self) -> &'static str {
        "keybindings"
    }

    fn title(&self) -> String {
        "Keybindings".to_string()
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        self.area = area;
        self.draw(area, buf, ctx);
    }

    fn captures_keys(&self) -> bool {
        self.capturing
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        if self.capturing {
            self.captured(&chord);
            return vec![PanelEvent::NeedsRedraw];
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
            + crate::settings::SEARCH_LABEL.len() as u16
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
