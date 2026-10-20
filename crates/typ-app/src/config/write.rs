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
        None => remove_keeping_comments(&mut document, key),
    }

    save(path, &document.to_string()).map_err(|e| named(&e))
}

/// Remove `key`, and keep the comment lines above it.
///
/// `toml_edit` files a comment above a key as that key's prefix, so a plain
/// remove deletes the note at the top of the file along with the first
/// setting under it. The prefix moves to the key that now stands in its
/// place, or to the end of the file when there is none.
fn remove_keeping_comments(document: &mut toml_edit::DocumentMut, key: &str) {
    let table = document.as_table_mut();
    let Some(at) = table.iter().position(|(k, _)| k == key) else {
        return;
    };
    let prefix = table
        .key(key)
        .and_then(|k| k.leaf_decor().prefix())
        .and_then(|p| p.as_str())
        .filter(|p| p.contains('#'))
        .map(str::to_string);
    table.remove(key);
    let Some(prefix) = prefix else {
        return;
    };
    let next = table.iter_mut().nth(at);
    match next {
        Some((mut next, item)) if item.is_value() => {
            let theirs = next
                .leaf_decor()
                .prefix()
                .and_then(|p| p.as_str())
                .unwrap_or_default()
                .to_string();
            next.leaf_decor_mut()
                .set_prefix(format!("{prefix}{theirs}"));
        }
        _ => {
            let trailing = document.trailing().as_str().unwrap_or_default().to_string();
            document.set_trailing(format!("{trailing}{prefix}"));
        }
    }
}

/// Put `config.toml` back as it was: `text`, or no file at all. What undo in
/// the settings tab writes, through the same temp file and rename.
pub(crate) fn restore_config(path: &Path, text: Option<&str>) -> Result<(), String> {
    let result = match text {
        Some(text) => save(path, text),
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    };
    result.map_err(|e| format!("{}: {e}", path.display()))
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
