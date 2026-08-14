use std::collections::HashMap;
use std::path::Path;

use typ_core::HandlerId;

/// The fallback used when no handler claims a path.
pub const EDITOR: HandlerId = HandlerId("editor");

/// Maps file extensions to the panel type that opens them.
///
/// This is the seam that keeps `PanelEvent` small: a new viewer registers here
/// rather than adding an enum variant, and the same path will later admit
/// externally provided handlers without any core change.
pub struct Registry {
    by_extension: HashMap<String, HandlerId>,
}

impl Registry {
    pub fn with_builtins() -> Self {
        // One content panel ships today. Entries exist so the mechanism is
        // exercised from day one rather than bolted on later.
        let mut by_extension = HashMap::new();
        for ext in ["rs", "toml", "md", "txt", "json", "yaml", "yml"] {
            by_extension.insert(ext.to_string(), EDITOR);
        }
        Self { by_extension }
    }

    pub fn register(&mut self, ext: &'static str, handler: HandlerId) {
        self.by_extension.insert(ext.to_lowercase(), handler);
    }

    pub fn handler_for(&self, path: &Path) -> HandlerId {
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .and_then(|e| self.by_extension.get(&e).copied())
            .unwrap_or(EDITOR)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_builtins()
    }
}
