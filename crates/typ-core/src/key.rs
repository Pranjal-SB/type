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
