//! Finding a theme by name and bringing it down to the terminal's depth.

use std::path::Path;

use typ_core::{Depth, Theme, ThemeColors, colour::downgrade_theme};

/// The themes that ship inside the binary.
///
/// **Embedded rather than installed beside the executable.** Helix keeps its
/// themes in a `runtime/` directory found through a five-step search — dev
/// sibling, user config, `HELIX_RUNTIME`, a build-time default, then
/// executable-relative — and it fails often enough that `hx --health` exists
/// partly to debug it and their own escape hatch is
/// `cargo install --features embed_runtime`. `cargo install typ-editor` has no
/// runtime directory to find, and the cold-start budget has no appetite for a
/// five-step path search either.
const EMBEDDED: &[(&str, &str)] = &[("slate", include_str!("themes/slate.toml"))];

/// Every theme name that ships, for listing and for error messages.
pub fn embedded_names() -> impl Iterator<Item = &'static str> {
    EMBEDDED.iter().map(|(name, _)| *name)
}

/// Load `name`, degraded to `depth`.
///
/// A file in `<config_dir>/themes/<name>.toml` wins over the embedded theme of
/// the same name, which is what makes "copy a shipped theme and edit it" work
/// and the only reason the embedded set is not a closed list.
///
/// Returns the shipped palette and a warning if anything went wrong. A theme
/// problem is never a startup failure, for the reason `keys.toml` established:
/// an editor that refuses to open because of a bad colour is an editor you
/// cannot use to fix the colour.
///
/// Returns `ThemeColors` rather than the whole `Theme` because nothing reads
/// the syntax scopes yet. Parsing still validates them, so a broken `[syntax]`
/// section is caught now rather than in M2.6 when the highlighter first looks.
pub fn load_theme(
    config_dir: Option<&Path>,
    name: &str,
    depth: Depth,
) -> (ThemeColors, Option<String>) {
    let (source, origin) = match find(config_dir, name) {
        Ok(found) => found,
        Err(warning) => return (ThemeColors::default(), Some(warning)),
    };

    match Theme::from_toml(&source) {
        // Degraded here, once, before anything downstream sees it. That is what
        // keeps `render.rs`, `gutter.rs` and `status.rs` taking a `ThemeColors`
        // and staying unaware that colour depth exists — and it is why
        // degradation is a function over a palette rather than data in a theme
        // file, which would make six shipped themes into eighteen.
        Ok(theme) => (downgrade_theme(&theme.colors, depth), None),
        Err(e) => (ThemeColors::default(), Some(format!("{origin}: {e:#}"))),
    }
}

/// The theme's source text, and something to name it by in a message.
fn find(config_dir: Option<&Path>, name: &str) -> Result<(String, String), String> {
    if let Some(dir) = config_dir {
        let path = dir.join("themes").join(format!("{name}.toml"));
        if path.exists() {
            return match std::fs::read_to_string(&path) {
                Ok(source) => Ok((source, path.display().to_string())),
                Err(e) => Err(format!("{}: {e}", path.display())),
            };
        }
    }

    EMBEDDED
        .iter()
        .find(|(embedded, _)| *embedded == name)
        .map(|(_, source)| ((*source).to_string(), format!("the built-in {name} theme")))
        .ok_or_else(|| {
            let known: Vec<&str> = embedded_names().collect();
            format!(
                "there is no theme called {name:?} — try one of: {}",
                known.join(", ")
            )
        })
}
