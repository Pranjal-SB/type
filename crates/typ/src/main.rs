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

    if !target.exists() {
        bail!("{} does not exist", target.display());
    }

    let (root, file) = if target.is_dir() {
        (target.clone(), None)
    } else {
        let parent = target.parent().unwrap_or(Path::new(".")).to_path_buf();
        (parent, Some(target.clone()))
    };

    let mut app = App::new(&root)?;

    // A broken keys.toml warns and starts on the defaults rather than refusing
    // to open — otherwise the one tool that could fix the typo is the one the
    // typo locked you out of.
    let (keymap, warning) = typ_app::config::load_keymap(typ_app::config::config_path().as_deref());
    app.set_keymap(keymap);
    if let Some(warning) = warning {
        app.notify(warning);
    }

    if let Some(f) = file {
        app.open_path(&f)?;
    }

    // Blocks until the user exits. No daemon detach — a caller waiting on
    // $EDITOR must see this process end when editing ends.
    run(app)
}
