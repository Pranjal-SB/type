//! Warp labels (interface §5): `ctrl+k j` labels every mouse target, and
//! typing a label clicks it.
//!
//! The click is synthesised and goes through `run::step` like any other, at
//! the target's point. So the keyboard path is the mouse path and the two
//! cannot drift: a target that stops doing something on a click stops doing
//! it from its label too, and the targets' own test catches both.

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::{AppEvent, KeyChord};

use crate::app::App;
use crate::app::targets::Target;

/// The second (and third) character of a label, in order: digits first, as
/// `t1`, `t2` read, then letters past nine.
const SYMBOLS: &[u8] = b"123456789abcdefghijklmnopqrstuvwxyz";

/// What the left of the status bar says while the labels are up.
const WARP_HINT: &str = "Warp: type a label  \u{b7}  Esc cancels";

/// The labels on screen and what has been typed of one.
pub(crate) struct Warp {
    labels: Vec<(String, Target)>,
    typed: String,
}

/// Every target labelled: the region letter, then its place in screen order
/// among that region's targets. A region with more targets than there are
/// symbols gets three-letter labels throughout, so no label is a prefix of
/// another (interface §11's fallback).
fn labelled(targets: Vec<Target>) -> Vec<(String, Target)> {
    let mut regions: Vec<char> = Vec::new();
    for target in &targets {
        if !regions.contains(&target.region()) {
            regions.push(target.region());
        }
    }
    let mut out = Vec::with_capacity(targets.len());
    for region in regions {
        let mut here: Vec<Target> = targets
            .iter()
            .copied()
            .filter(|t| t.region() == region)
            .collect();
        here.sort_by_key(|t| (t.rect.y, t.rect.x));
        let wide = here.len() > SYMBOLS.len();
        for (n, target) in here.into_iter().enumerate() {
            let mut label = region.to_string();
            if wide {
                label.push(SYMBOLS[n / SYMBOLS.len() % SYMBOLS.len()] as char);
            }
            label.push(SYMBOLS[n % SYMBOLS.len()] as char);
            out.push((label, target));
        }
    }
    out
}

impl App {
    /// The labels on screen, while they are up.
    pub fn warp_labels(&self) -> Option<&[(String, Target)]> {
        self.warp.as_ref().map(|warp| warp.labels.as_slice())
    }

    /// Put the labels up over the last frame's targets.
    pub(super) fn open_warp(&mut self) {
        let labels = labelled(self.targets(self.screen));
        if labels.is_empty() {
            self.status = Some("Nothing on screen to warp to.".to_string());
            return;
        }
        self.warp = Some(Warp {
            labels,
            typed: String::new(),
        });
        self.mark_dirty();
    }

    /// The status bar's left half while the labels are up.
    pub(super) fn warp_hint(&self) -> Option<String> {
        let warp = self.warp.as_ref()?;
        Some(match warp.typed.as_str() {
            "" => WARP_HINT.to_string(),
            typed => format!("{WARP_HINT}  {typed}"),
        })
    }

    /// A key while the labels are up. A character that continues a label is
    /// kept, one that completes it clicks the target; `esc` cancels, and
    /// anything else cancels and says what did not match.
    pub(super) fn handle_warp_chord(&mut self, chord: &KeyChord) -> Result<()> {
        let Some(mut warp) = self.warp.take() else {
            return Ok(());
        };
        self.mark_dirty();
        let plain = !chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let c = match chord.raw.code {
            KeyCode::Esc => return Ok(()),
            KeyCode::Char(c) if plain => c.to_ascii_lowercase(),
            _ => {
                self.status = Some(format!("{} is not a warp label.", chord.canonical));
                return Ok(());
            }
        };
        warp.typed.push(c);
        if let Some((_, target)) = warp.labels.iter().find(|(l, _)| *l == warp.typed) {
            return self.click_at(target.point);
        }
        if warp.labels.iter().any(|(l, _)| l.starts_with(&warp.typed)) {
            self.warp = Some(warp);
        } else {
            self.status = Some(format!("No warp label {}.", warp.typed));
        }
        Ok(())
    }

    /// A left press at `point`, through the same path a mouse takes.
    fn click_at(&mut self, (column, row): (u16, u16)) -> Result<()> {
        let event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        crate::run::step(self, AppEvent::Input(Event::Mouse(event)), self.screen).map(drop)
    }

    /// The labels, over a body stepped back like the picker's (interface §3),
    /// the status bar left as it is. Only the labels that what has been typed
    /// still leads to.
    pub(super) fn render_warp(&self, frame: Rect, buf: &mut ratatui::buffer::Buffer) {
        let Some(warp) = &self.warp else {
            return;
        };
        let (behind, _) = crate::layout::split_frame(frame);
        typ_core::chrome::scrim(buf, behind, self.theme.scrim);
        let style = Style::default()
            .fg(self.theme.status_bar_accent)
            .bg(self.theme.chrome_bg)
            .add_modifier(Modifier::BOLD);
        for (label, target) in &warp.labels {
            if !label.starts_with(&warp.typed) {
                continue;
            }
            let room = frame.right().saturating_sub(target.rect.x) as usize;
            buf.set_stringn(target.rect.x, target.rect.y, label, room, style);
        }
    }
}
