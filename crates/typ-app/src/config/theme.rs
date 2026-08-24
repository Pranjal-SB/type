//! Finding a theme by name and bringing it down to the terminal's depth.

use std::path::Path;

use typ_core::{Depth, SyntaxTheme, Theme, ThemeColors, colour::downgrade_theme};

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
const EMBEDDED: &[(&str, &str)] = &[
    ("slate", include_str!("themes/slate.toml")),
    ("mocha", include_str!("themes/mocha.toml")),
    ("latte", include_str!("themes/latte.toml")),
    ("dracula", include_str!("themes/dracula.toml")),
    ("rose-pine", include_str!("themes/rose-pine.toml")),
    ("tokyo-night", include_str!("themes/tokyo-night.toml")),
];

/// Every theme name that ships, for listing and for error messages.
pub fn embedded_names() -> impl Iterator<Item = &'static str> {
    EMBEDDED.iter().map(|(name, _)| *name)
}

/// Every shipped theme, name and source.
///
/// Exists so the contrast check can enumerate what ships rather than being
/// handed a list — a test naming six themes goes on passing forever after
/// somebody adds a seventh.
pub fn embedded() -> impl Iterator<Item = (&'static str, &'static str)> {
    EMBEDDED.iter().copied()
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
/// Returns both halves of the theme. The syntax scopes were parsed and thrown
/// away until M2.7, when the highlighter arrived to read them.
pub fn load_theme(
    config_dir: Option<&Path>,
    name: &str,
    depth: Depth,
) -> (ThemeColors, SyntaxTheme, Option<String>) {
    let (source, origin) = match find(config_dir, name) {
        Ok(found) => found,
        Err(warning) => {
            return (
                ThemeColors::default(),
                SyntaxTheme::default(),
                Some(warning),
            );
        }
    };

    match Theme::from_toml(&source) {
        // Degraded here, once, before anything downstream sees it. That is what
        // keeps `render.rs`, `gutter.rs` and `status.rs` taking a `ThemeColors`
        // and staying unaware that colour depth exists — and it is why
        // degradation is a function over a palette rather than data in a theme
        // file, which would make six shipped themes into eighteen.
        //
        // The scopes take the same trip. A `[syntax]` table left in true colour
        // while the palette around it was quantised would be the one part of
        // the screen sending escape codes the terminal cannot honour.
        Ok(theme) => (
            downgrade_theme(&theme.colors, depth),
            theme.syntax.downgraded(depth),
            None,
        ),
        Err(e) => (
            ThemeColors::default(),
            SyntaxTheme::default(),
            Some(format!("{origin}: {e:#}")),
        ),
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
