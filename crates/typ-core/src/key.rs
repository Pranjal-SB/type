use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key press in both raw and canonical form.
///
/// `raw` is used for text insertion and PTY passthrough, where the exact event
/// matters. `canonical` is used for keybinding lookup, where a stable string
/// form matters. Keeping both avoids the bug where a binding table and a
/// text-input path disagree about what was pressed.
#[derive(Debug, Clone)]
pub struct KeyChord {
    pub raw: KeyEvent,
    pub canonical: String,
}

impl KeyChord {
    pub fn from_event(raw: KeyEvent) -> Self {
        let mut s = String::new();
        // Fixed order so a binding table never has to guess.
        if raw.modifiers.contains(KeyModifiers::CONTROL) {
            s.push_str("ctrl+");
        }
        if raw.modifiers.contains(KeyModifiers::ALT) {
            s.push_str("alt+");
        }
        if raw.modifiers.contains(KeyModifiers::SHIFT) {
            s.push_str("shift+");
        }
        s.push_str(&key_name(raw.code));
        Self { raw, canonical: s }
    }
}

/// Keys `key_name` spells as a word, which a config file may name.
const NAMED_KEYS: &[&str] = &[
    "enter",
    "esc",
    "tab",
    "backtab",
    "backspace",
    "delete",
    "insert",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
];

/// A chord as a config file spells it, in the form `KeyChord::canonical` takes.
///
/// `lookup` compares strings, so a binding written `Ctrl+S` or `shift+ctrl+p`
/// used to load without complaint and never fire. Case and modifier order are
/// forgiven here; a key or modifier that does not exist is an error, because a
/// binding that can never fire is a config mistake the user needs to hear
/// about. Gap 100.
pub(crate) fn canonical_chord(spelling: &str) -> Result<String, String> {
    let lower = spelling.to_lowercase();
    // `+` is itself a key, so `ctrl++` ends in an empty segment after the
    // last separator.
    let (mods, key) = match lower.strip_suffix("++") {
        Some(mods) => (mods, "+"),
        None if lower == "+" => ("", "+"),
        None => lower.rsplit_once('+').unwrap_or(("", lower.as_str())),
    };

    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for modifier in mods.split('+').filter(|m| !m.is_empty()) {
        match modifier {
            "ctrl" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            other => {
                return Err(format!(
                    "{spelling}: {other:?} is not a modifier (ctrl, alt, shift)"
                ));
            }
        }
    }

    let is_function_key = key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    if !(key.chars().count() == 1 || is_function_key || NAMED_KEYS.contains(&key)) {
        return Err(format!("{spelling}: {key:?} is not a key"));
    }

    let mut canonical = String::new();
    for (on, name) in [(ctrl, "ctrl+"), (alt, "alt+"), (shift, "shift+")] {
        if on {
            canonical.push_str(name);
        }
    }
    canonical.push_str(key);
    Ok(canonical)
}

fn key_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "backtab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}
