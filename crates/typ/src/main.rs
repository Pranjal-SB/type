use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};
use typ_app::{App, run::run};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
typ — TYPE, the Terminal-Yoked Programming Environment

USAGE:
    typ [PATH]

ARGS:
    PATH    File to open, or directory to open as a workspace.
            Defaults to the current directory.

OPTIONS:
    -h, --help       Print this help
    -V, --version    Print version
";

/// What a command-line path turned out to mean.
#[derive(Debug)]
struct Target {
    /// The workspace the file tree shows.
    root: PathBuf,
    /// The file to open, if one was named.
    file: Option<PathBuf>,
}

/// Decide what a path means: a workspace, an existing file, or a file to create.
///
/// `typ notes.md` on a path that does not exist opens an empty buffer that
/// `save` will create, which is what every editor in the field does and what
/// TYPE refused to do until now.
///
/// A **missing parent directory is still an error**, and deliberately so. The
/// alternative is a user typing into a buffer that can never be saved, and
/// finding out only when they try — so this fails before the alternate screen
/// is ever entered, while stderr is still visible.
fn resolve(target: &Path) -> Result<Target> {
    if target.is_dir() {
        return Ok(Target {
            root: target.to_path_buf(),
            file: None,
        });
    }

    // An empty parent means a bare filename — `typ notes.md` — whose directory
    // is the one we are standing in.
    let parent = match target.parent() {
        Some(p) if p.as_os_str().is_empty() => PathBuf::from("."),
        Some(p) => p.to_path_buf(),
        None => PathBuf::from("."),
    };

    if !target.exists() && !parent.is_dir() {
        bail!("{} does not exist", target.display());
    }

    Ok(Target {
        root: parent,
        file: Some(target.to_path_buf()),
    })
}

/// Exit codes are load-bearing: TYPE is usable as `$EDITOR`, and a caller such
/// as `git commit` must abort when the editor fails rather than proceeding with
/// an empty message.
fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("typ: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<()> {
    // First, before anything that can fail. Initialising after argument
    // handling looked fine and left the one path that most needs a log entry —
    // a bad path, which exits non-zero and takes `$EDITOR`'s caller down with
    // it — writing nothing at all.
    typ_app::log::init_from_env();

    let args: Vec<String> = std::env::args().skip(1).collect();

    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("typ {VERSION}");
                return Ok(());
            }
            _ => {}
        }
    }

    let target: PathBuf = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let Target { root, file } = resolve(&target).inspect_err(|e| {
        typ_app::log_error!("{} could not be opened: {e:#}", target.display());
    })?;

    // The clipboard only reaches outside this process once a binary says so.
    // Defaulting it off means a test suite linking typ-buffer never spawns
    // wl-copy and never clobbers what the developer had copied.
    typ_buffer::clipboard::enable_system();

    let mut app = App::new(&root)?;

    // A broken keys.toml warns and starts on the defaults rather than refusing
    // to open — otherwise the one tool that could fix the typo is the one the
    // typo locked you out of.
    typ_app::log_info!("typ {VERSION} starting, root {}", root.display());
    // The single most useful line when somebody reports that copy does nothing.
    typ_app::log_info!(
        "clipboard provider: {}",
        typ_buffer::clipboard::provider_name()
    );
    // Every config file is loaded the same way: take what parsed, collect what
    // did not, and start regardless.
    let mut complaints: Vec<String> = Vec::new();

    let (settings, warning) =
        typ_app::config::load_settings(typ_app::config::settings_path().as_deref());
    complaints.extend(warning);

    // Before any file is opened, so the first one is affected too.
    app.set_indent_width(settings.indent_width);
    app.set_whitespace(settings.whitespace);

    let (keymap, warning) = typ_app::config::load_keymap(typ_app::config::config_path().as_deref());
    app.set_keymap(keymap);
    complaints.extend(warning);

    // The setting wins over detection where it is set, because nothing in the
    // environment separates a tmux that forwards truecolor from one that
    // mangles it.
    let depth = settings
        .color_depth
        .unwrap_or_else(typ_app::capability::detect);
    // The line to look at when somebody reports that the colours are wrong.
    typ_app::log_info!("colour depth: {depth:?}, theme: {}", settings.theme);

    let (colors, warning) = typ_app::config::load_theme(
        typ_app::config::config_dir().as_deref(),
        &settings.theme,
        depth,
    );
    app.set_theme(colors);
    complaints.extend(warning);

    if !complaints.is_empty() {
        for complaint in &complaints {
            typ_app::log_warn!("{complaint}");
        }
        // All of them, not the first: two broken config files is exactly when
        // hearing about only one wastes the most time.
        app.notify(complaints.join("  ·  "));
    }

    if let Some(f) = file {
        app.open_path(&f)?;
    }

    // Blocks until the user exits. No daemon detach — a caller waiting on
    // $EDITOR must see this process end when editing ends.
    run(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own.
    ///
    /// Created, never deleted first. On Windows `remove_dir_all` can return
    /// before the directory is actually gone, so removing and immediately
    /// recreating fails intermittently — which is exactly how it failed once
    /// here. The name is unique per test, so there is nothing stale to clear.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("typ-resolve").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_directory_is_a_workspace_with_nothing_open() {
        let dir = temp("workspace");
        let target = resolve(&dir).unwrap();
        assert_eq!(target.root, dir);
        assert_eq!(target.file, None);
    }

    #[test]
    fn an_existing_file_opens_with_its_directory_as_the_workspace() {
        let dir = temp("existing");
        let file = dir.join("there.rs");
        std::fs::write(&file, "fn main() {}\n").unwrap();

        let target = resolve(&file).unwrap();

        assert_eq!(target.root, dir);
        assert_eq!(target.file, Some(file));
    }

    #[test]
    fn a_missing_file_in_a_real_directory_is_a_file_to_create() {
        let dir = temp("to-create");
        let file = dir.join("new.rs");
        let _ = std::fs::remove_file(&file);

        let target = resolve(&file).unwrap();

        assert_eq!(target.root, dir);
        assert_eq!(target.file, Some(file.clone()));
        assert!(!file.exists(), "resolving must not create anything");
    }

    #[test]
    fn a_missing_parent_directory_is_still_an_error() {
        let dir = temp("no-parent");
        let file = dir.join("nowhere").join("new.rs");

        let error = resolve(&file).unwrap_err();

        assert!(
            error.to_string().contains("does not exist"),
            "a buffer that can never be saved must fail before the screen is taken, got: {error}"
        );
    }
}
