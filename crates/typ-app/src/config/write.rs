//! Writing one setting into `config.toml`, keeping every other byte.
//!
//! Through `toml_edit` rather than `toml`: `toml` parses and serialises but
//! drops comments, and a settings change that deletes the user's notes is a
//! change nobody asked for.

use std::path::Path;

/// Set `key` to `value` in the file at `path`, or remove it when `value` is
/// `None`. Creates the file when there is none.
///
/// **A file that does not parse is refused, not replaced.** Its text is the
/// user's to fix; writing a fresh one over it would throw that text away.
///
/// Through a temp file and a rename, as `session::save` is, so a failure
/// part way leaves the old file rather than half of a new one. The write is
/// one small file, done on the render thread: a few hundred bytes, the same
/// order as a save.
pub fn write_setting(
    path: &Path,
    key: &str,
    value: Option<toml_edit::Value>,
) -> Result<(), String> {
    let named = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let source = super::read_config(path)?.unwrap_or_default();
    let mut document: toml_edit::DocumentMut = source.parse().map_err(|e| named(&e))?;

    match value {
        Some(value) => match document.get_mut(key) {
            // Replacing the value alone keeps the comment after it on the line.
            Some(item) if item.is_value() => {
                let decor = item.as_value().map(|old| old.decor().clone());
                let mut value = value;
                if let Some(decor) = decor {
                    *value.decor_mut() = decor;
                }
                *item = toml_edit::Item::Value(value);
            }
            _ => {
                document.insert(key, toml_edit::Item::Value(value));
            }
        },
        None => {
            document.remove(key);
        }
    }

    save(path, &document.to_string()).map_err(|e| named(&e))
}

/// Write `text` to `path` through a temp file and a rename.
fn save(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}
