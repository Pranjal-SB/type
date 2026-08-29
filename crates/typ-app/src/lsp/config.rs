//! Which server a file gets, and where it is rooted.
//!
//! Two questions, and the second is the one with a wrong answer that looks
//! right. Both were settled by reading the editors that already got them wrong
//! once rather than by reasoning from the specification, which says nothing
//! about either.

use std::path::{Path, PathBuf};

/// One language server, and what it is for.
///
/// Keyed by extension rather than by TYPE's `Language` enum: TYPE highlights
/// five languages and can talk to a server for any file at all, so the set of
/// things that can have a server is not the set of things that have a grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The `languageId` sent in `didOpen`. `rust`, `toml`, `python`.
    pub language_id: String,
    /// Extensions this server handles, without the dot.
    pub extensions: Vec<String>,
    /// The binary. Not found on `PATH` is the ordinary case, not an error.
    pub command: String,
    pub args: Vec<String>,
    /// Files whose presence marks a project root, checked in no order —
    /// any one of them is enough.
    pub roots: Vec<String>,
}

impl ServerConfig {
    pub(crate) fn handles(&self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

/// Markers that mean "a project starts here" for a language with nothing more
/// specific to say.
const COMMON_ROOTS: [&str; 5] = [
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    ".git",
];

/// The servers TYPE knows about without being told.
///
/// **Two, and both are single static Rust binaries.** Every other candidate for
/// the languages TYPE highlights needs node on the machine, and a default that
/// is silently absent is worse than no default at all. `taplo` also earns its
/// place by being shaped differently: it is the only default with arguments, so
/// it exercises that path against a real server rather than only against the
/// fake one.
pub(crate) fn defaults() -> Vec<ServerConfig> {
    vec![
        ServerConfig {
            language_id: "rust".into(),
            extensions: vec!["rs".into()],
            command: "rust-analyzer".into(),
            args: Vec::new(),
            // Only `Cargo.toml`. A `.git` fallback would root rust-analyzer at
            // a repository holding several unrelated crates, and `Cargo.toml`
            // is present in every Rust project there is.
            roots: vec!["Cargo.toml".into()],
        },
        ServerConfig {
            language_id: "toml".into(),
            extensions: vec!["toml".into()],
            command: "taplo".into(),
            // **The subcommand matters.** `taplo` with no arguments prints
            // help and exits, so a default with the wrong argv is a default
            // that silently never starts. Helix ships the same two words.
            args: vec!["lsp".into(), "stdio".into()],
            roots: COMMON_ROOTS.iter().map(|m| m.to_string()).collect(),
        },
    ]
}

/// Where a server for `file` should be rooted, given a project root it may not
/// leave.
///
/// **The outermost marker wins, not the nearest**, and this is the one place a
/// plausible rule is wrong in a way that costs a machine rather than a feature.
/// Given a Cargo workspace — which TYPE itself is — the nearest ancestor of
/// `crates/typ-app/src/app.rs` holding a `Cargo.toml` is `crates/typ-app`, and
/// the nearest for `crates/typ-core/src/lib.rs` is `crates/typ-core`. One
/// server per root then means **one rust-analyzer per open crate**, each
/// indexing the whole workspace. Eleven crates, eleven processes.
///
/// Both editors that had to solve this arrived at the same answer
/// independently. Helix's `find_lsp_workspace` overwrites `top_marker` as it
/// walks upward; Zed's `CargoManifestProvider::search` names its accumulator
/// `outermost_cargo_toml` and does the same thing. Neither takes the nearest.
///
/// **Bounded by `project_root`, which matters as much as the direction.** An
/// unbounded walk finds `$HOME/.git` and roots every project on the machine in
/// the home directory — one server, indexing everything.
pub(crate) fn root_for(file: &Path, markers: &[String], project_root: &Path) -> PathBuf {
    let mut outermost = None;

    for ancestor in file.ancestors().skip(1) {
        if !ancestor.starts_with(project_root) {
            break;
        }
        if markers.iter().any(|m| ancestor.join(m).exists()) {
            outermost = Some(ancestor.to_path_buf());
        }
        if ancestor == project_root {
            break;
        }
    }

    // A file with no marker above it belongs to the project it was opened in.
    // That is what VS Code and Zed do for a folder with nothing in it that
    // names a build system, and it keeps the count of servers at one.
    outermost.unwrap_or_else(|| project_root.to_path_buf())
}

/// Read `[[language]]` out of an already-parsed `config.toml`.
///
/// **Configured entries replace a default of the same `name`, and anything else
/// is added.** TYPE highlights five languages and can talk to a server for any
/// file at all, so a language it has never heard of is a configuration, not a
/// mistake.
///
/// Like the rest of `config.toml` and unlike a keymap: an entry that does not
/// parse is dropped with a complaint and the others still apply.
pub(crate) fn load(table: &toml::Table) -> (Vec<ServerConfig>, Vec<String>) {
    let mut servers = defaults();
    let mut complaints = Vec::new();

    let Some(entries) = table.get("language") else {
        return (servers, complaints);
    };
    let Some(entries) = entries.as_array() else {
        complaints.push("language must be written as [[language]] tables".to_string());
        return (servers, complaints);
    };

    for entry in entries {
        match parse_entry(entry) {
            Ok(config) => match servers
                .iter()
                .position(|s| s.language_id == config.language_id)
            {
                Some(existing) => servers[existing] = config,
                None => servers.push(config),
            },
            Err(complaint) => complaints.push(complaint),
        }
    }

    (servers, complaints)
}

fn parse_entry(entry: &toml::Value) -> Result<ServerConfig, String> {
    let table = entry
        .as_table()
        .ok_or_else(|| "a [[language]] entry must be a table".to_string())?;

    let name = table
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "a [[language]] entry needs a name in quotes".to_string())?;

    let command = table
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("language {name} needs a command in quotes"))?;

    let strings = |key: &str| -> Vec<String> {
        table
            .get(key)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    let extensions = strings("extensions");
    if extensions.is_empty() {
        // Without one, nothing would ever reach it — a silent no-op is worse
        // than a line saying the entry does nothing.
        return Err(format!("language {name} needs at least one extension"));
    }

    let roots = match strings("roots") {
        empty if empty.is_empty() => COMMON_ROOTS.iter().map(|m| m.to_string()).collect(),
        given => given,
    };

    Ok(ServerConfig {
        language_id: name.to_string(),
        extensions,
        command: command.to_string(),
        args: strings("args"),
        roots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("typ-lsp-roots").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn markers(list: &[&str]) -> Vec<String> {
        list.iter().map(|m| m.to_string()).collect()
    }

    #[test]
    fn the_outermost_marker_wins_not_the_nearest() {
        // A Cargo workspace, which is what TYPE is. The nearest `Cargo.toml`
        // above `crates/app/src/main.rs` is `crates/app`; taking it would give
        // this repository one rust-analyzer per crate.
        let root = dir("workspace");
        touch(&root.join("Cargo.toml"));
        touch(&root.join("crates/app/Cargo.toml"));
        let file = root.join("crates/app/src/main.rs");
        touch(&file);

        assert_eq!(root_for(&file, &markers(&["Cargo.toml"]), &root), root);
    }

    #[test]
    fn the_walk_stops_at_the_project_root() {
        // An unbounded walk finds `$HOME/.git` and roots every project on the
        // machine in one place.
        let outside = dir("bounded");
        touch(&outside.join(".git"));
        let project = outside.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let file = project.join("src/main.rs");
        touch(&file);

        assert_eq!(root_for(&file, &markers(&[".git"]), &project), project);
    }

    #[test]
    fn a_project_with_no_marker_is_rooted_where_it_was_opened() {
        let root = dir("bare");
        let file = root.join("notes/a.rs");
        touch(&file);
        assert_eq!(root_for(&file, &markers(&["Cargo.toml"]), &root), root);
    }

    #[test]
    fn a_marker_on_the_project_root_itself_counts() {
        let root = dir("at-root");
        touch(&root.join("Cargo.toml"));
        let file = root.join("src/main.rs");
        touch(&file);
        assert_eq!(root_for(&file, &markers(&["Cargo.toml"]), &root), root);
    }

    #[test]
    fn a_nested_project_below_the_root_keeps_its_own_root() {
        // Only the inner directory has a marker, so it is both outermost and
        // nearest — the case the two rules agree on.
        let root = dir("nested");
        let inner = root.join("vendor/thing");
        touch(&inner.join("Cargo.toml"));
        let file = inner.join("src/lib.rs");
        touch(&file);
        assert_eq!(root_for(&file, &markers(&["Cargo.toml"]), &root), inner);
    }

    #[test]
    fn rust_and_toml_are_configured_without_a_config_file() {
        let servers = defaults();
        let rust = servers.iter().find(|s| s.language_id == "rust").unwrap();
        assert_eq!(rust.command, "rust-analyzer");
        assert!(rust.args.is_empty());

        let toml = servers.iter().find(|s| s.language_id == "toml").unwrap();
        assert_eq!(toml.command, "taplo");
        assert_eq!(toml.args, ["lsp", "stdio"], "the subcommand is the default");
    }

    #[test]
    fn a_config_entry_replaces_a_default_of_the_same_name() {
        let table: toml::Table = toml::from_str(
            r#"
            [[language]]
            name = "rust"
            extensions = ["rs"]
            command = "rust-analyzer-nightly"
            "#,
        )
        .unwrap();
        let (servers, complaints) = load(&table);
        assert!(complaints.is_empty(), "{complaints:?}");
        let rust: Vec<&ServerConfig> = servers.iter().filter(|s| s.language_id == "rust").collect();
        assert_eq!(rust.len(), 1, "the default was left behind beside it");
        assert_eq!(rust[0].command, "rust-analyzer-nightly");
    }

    #[test]
    fn a_language_type_has_never_heard_of_is_added_not_rejected() {
        let table: toml::Table = toml::from_str(
            r#"
            [[language]]
            name = "zig"
            extensions = ["zig"]
            command = "zls"
            "#,
        )
        .unwrap();
        let (servers, complaints) = load(&table);
        assert!(complaints.is_empty(), "{complaints:?}");
        assert!(servers.iter().any(|s| s.language_id == "zig"));
        assert_eq!(servers.len(), defaults().len() + 1);
    }

    #[test]
    fn an_entry_missing_its_command_is_dropped_with_a_complaint() {
        let table: toml::Table = toml::from_str(
            r#"
            [[language]]
            name = "zig"
            extensions = ["zig"]
            "#,
        )
        .unwrap();
        let (servers, complaints) = load(&table);
        assert_eq!(servers.len(), defaults().len(), "it was added anyway");
        assert_eq!(complaints.len(), 1, "{complaints:?}");
    }

    #[test]
    fn an_entry_with_no_extensions_is_dropped_because_nothing_could_reach_it() {
        let table: toml::Table = toml::from_str(
            r#"
            [[language]]
            name = "zig"
            command = "zls"
            "#,
        )
        .unwrap();
        let (_, complaints) = load(&table);
        assert_eq!(complaints.len(), 1, "{complaints:?}");
    }

    #[test]
    fn arguments_and_roots_come_through() {
        let table: toml::Table = toml::from_str(
            r#"
            [[language]]
            name = "zig"
            extensions = ["zig"]
            command = "zls"
            args = ["--stdio"]
            roots = ["build.zig"]
            "#,
        )
        .unwrap();
        let (servers, _) = load(&table);
        let zig = servers.iter().find(|s| s.language_id == "zig").unwrap();
        assert_eq!(zig.args, ["--stdio"]);
        assert_eq!(zig.roots, ["build.zig"]);
    }

    #[test]
    fn no_language_table_leaves_the_defaults_alone() {
        let table: toml::Table = toml::from_str("theme = \"slate\"").unwrap();
        let (servers, complaints) = load(&table);
        assert_eq!(servers, defaults());
        assert!(complaints.is_empty());
    }
}
