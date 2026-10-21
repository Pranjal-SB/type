//! What servers say they are busy with, from `$/progress`.
//!
//! Split from `mod.rs` at M4.6: the status bar's one source of server work,
//! and nothing else in the module reads it.

use typ_lsp::ServerId;

use super::Lsp;

impl Lsp {
    /// What the servers are busy with, oldest token first.
    pub(crate) fn progress(&self) -> Vec<&str> {
        self.progress
            .iter()
            .map(|(_, text)| text.as_str())
            .collect()
    }

    /// The entry for a token, if it has begun.
    fn progress_slot(&mut self, key: &(ServerId, String)) -> Option<&mut String> {
        self.progress
            .iter_mut()
            .find(|(existing, _)| existing == key)
            .map(|(_, text)| text)
    }

    /// Record a `$/progress` notification. Returns whether the bar changed.
    ///
    /// Three kinds. `begin` names the work, `report` refines it, `end` removes
    /// it, and a `report` for a token nobody began is ignored rather than
    /// invented, because a percentage with no title is a number with no noun.
    pub(crate) fn note_progress(&mut self, server: ServerId, params: &serde_json::Value) -> bool {
        let Some(token) = params.get("token").and_then(token_name) else {
            return false;
        };
        let Some(value) = params.get("value") else {
            return false;
        };
        let key = (server, token);

        match value.get("kind").and_then(|k| k.as_str()) {
            Some("begin") => {
                let title = value
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Working")
                    .to_string();
                let text = describe(&title, value);
                match self.progress_slot(&key) {
                    Some(existing) => *existing = text,
                    None => self.progress.push((key, text)),
                }
                true
            }
            Some("report") => {
                let Some(existing) = self.progress_slot(&key) else {
                    // A report for a token nobody began is a percentage with no
                    // noun. Inventing a title for it would put a made-up word
                    // on the bar.
                    return false;
                };
                // The title came with `begin` and `report` does not repeat it,
                // so it is carried forward from what is already shown.
                let title = existing
                    .split_whitespace()
                    .next()
                    .unwrap_or("Working")
                    .to_string();
                *existing = describe(&title, value);
                true
            }
            Some("end") => {
                let before = self.progress.len();
                self.progress.retain(|(existing, _)| existing != &key);
                self.progress.len() != before
            }
            _ => false,
        }
    }

    /// Forget everything a server said it was doing.
    pub(crate) fn forget_progress(&mut self, server: ServerId) {
        self.progress.retain(|((id, _), _)| *id != server);
    }
}

/// A progress token, which the protocol allows to be a string or a number.
fn token_name(token: &serde_json::Value) -> Option<String> {
    match token {
        serde_json::Value::String(name) => Some(name.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// One line of progress: the title, and the most specific thing said about it.
///
/// A percentage when there is one, a message otherwise, nothing when the server
/// offered neither, which is the case where a title on its own is the whole
/// truth.
fn describe(title: &str, value: &serde_json::Value) -> String {
    if let Some(percentage) = value.get("percentage").and_then(|p| p.as_u64()) {
        return format!("{title} {percentage}%");
    }
    match value.get("message").and_then(|m| m.as_str()) {
        Some(message) if !message.is_empty() => format!("{title} {message}"),
        _ => title.to_string(),
    }
}
