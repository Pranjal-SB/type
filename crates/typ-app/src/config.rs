//! Finding and loading user config.
//!
//! Config problems are warnings, never startup failures. An editor that refuses
//! to open because of a typo in a keybinding is an editor you cannot use to fix
//! the typo.

use std::path::{Path, PathBuf};

use typ_core::Keymap;

/// `$TYP_CONFIG_DIR/keys.toml` if set, else the platform config directory.
///
/// The environment variable exists so tests and `$EDITOR` invocations can be
/// isolated from whatever the developer has in their real config.
pub fn config_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TYP_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("keys.toml"));
    }
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()?
    } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        xdg
    } else {
        format!("{}/.config", std::env::var("HOME").ok()?)
    };
    Some(PathBuf::from(base).join("typ").join("keys.toml"))
}

/// The keymap, plus a warning if the config existed and could not be used.
pub fn load_keymap(path: Option<&Path>) -> (Keymap, Option<String>) {
    let mut keymap = Keymap::default_bindings();
    let Some(path) = path else {
        return (keymap, None);
    };
    let Ok(source) = std::fs::read_to_string(path) else {
        // No config is the normal case, not a problem worth a message.
        return (keymap, None);
    };
    match keymap.merge_toml(&source) {
        Ok(()) => (keymap, None),
        Err(e) => {
            // `merge_toml` is all-or-nothing, so `keymap` is still the untouched
            // defaults here. Rebuilding them is belt and braces against that
            // guarantee ever weakening without this line being revisited.
            let warning = format!("{}: {e:#}", path.display());
            (Keymap::default_bindings(), Some(warning))
        }
    }
}
