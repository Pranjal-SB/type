//! Loading `keys.toml`.

use std::path::Path;

use typ_core::Keymap;

/// The keymap, plus a warning if the config existed and could not be used.
pub fn load_keymap(path: Option<&Path>) -> (Keymap, Option<String>) {
    let mut keymap = Keymap::default_bindings();
    let Some(path) = path else {
        return (keymap, None);
    };
    let source = match super::read_config(path) {
        Ok(Some(source)) => source,
        // No config is the normal case, not a problem worth a message.
        Ok(None) => return (keymap, None),
        Err(warning) => return (keymap, Some(warning)),
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
