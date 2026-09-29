//! Finding and loading user config.
//!
//! Config problems are warnings, never startup failures. An editor that refuses
//! to open because of a typo in a keybinding is an editor you cannot use to fix
//! the typo — and the same goes for a colour.
//!
//! Three files, each loaded by its own module:
//!
//! | File | What |
//! |---|---|
//! | `keys.toml` | keybindings |
//! | `config.toml` | everything else — the theme's name, colour depth |
//! | `themes/<name>.toml` | a theme, overriding an embedded one of the same name |

use std::path::PathBuf;

pub mod keys;
pub mod settings;
pub mod theme;

pub use keys::load_keymap;
pub use settings::{Settings, load_settings};
pub use theme::{embedded_names, load_theme};

/// Where user config lives, if the platform will say.
///
/// `TYP_CONFIG_DIR` exists so tests and `$EDITOR` invocations can be isolated
/// from whatever the developer has in their real config.
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TYP_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()?
    } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        xdg
    } else {
        format!("{}/.config", std::env::var("HOME").ok()?)
    };
    Some(PathBuf::from(base).join("typ"))
}

/// Read a config file. `Ok(None)` when there is no file, which is the normal
/// case; `Err` with a message naming the file for anything else.
///
/// **Only `NotFound` is absence.** A permission problem, a directory where the
/// file should be, or a file saved as UTF-16 used to read as "no config", so the
/// user's settings silently did not apply. Gap 97.
pub(crate) fn read_config(path: &std::path::Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// `keys.toml` in the config directory.
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("keys.toml"))
}

/// `config.toml` in the config directory.
pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}
