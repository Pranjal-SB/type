//! Every publishable crate carries the metadata crates.io requires.
//!
//! `typ-syntax` shipped in M2.7 without `description`, `keywords` or
//! `categories`, and nothing noticed until `cargo publish --dry-run` refused it
//! during the v0.2.7 release — after the tag was cut and the binaries were
//! built. The failure is invisible until the one moment it blocks a release,
//! which is the worst time to find it.
//!
//! It lives here rather than in `docs/releasing.md` because a checklist item is
//! only as good as the person reading it, and M3 adds another crate.
//!
//! A text search rather than a TOML parse: the fields are inherited from the
//! workspace (`keywords.workspace = true`), so what matters is that the line is
//! present at all, and asserting that needs no dependency.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root resolves")
}

fn member_manifests() -> Vec<PathBuf> {
    fs::read_dir(workspace_root().join("crates"))
        .expect("crates/ exists")
        .filter_map(|entry| {
            let path = entry.ok()?.path().join("Cargo.toml");
            path.is_file().then_some(path)
        })
        .collect()
}

#[test]
fn every_crate_declares_what_crates_io_requires() {
    let manifests = member_manifests();
    assert!(
        manifests.len() >= 8,
        "found {} manifests — the glob stopped matching",
        manifests.len()
    );

    let mut missing = Vec::new();
    for manifest in manifests {
        let text = fs::read_to_string(&manifest).expect("manifest reads");
        for field in ["description", "keywords", "categories"] {
            if !text.lines().any(|line| line.starts_with(field)) {
                missing.push(format!("{}: {field}", manifest.display()));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "crates.io rejects a package without these; publish would fail mid-release:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn every_path_dependency_names_the_workspace_version() {
    // A path dependency carries a version because cargo refuses to publish a
    // crate whose dependencies are path-only — and a *stale* one publishes a
    // package that cannot resolve, which is only discovered by a stranger
    // trying to install it.
    //
    // The bump is a search-and-replace across nine lines and it has already
    // been done wrong once: a regex that stopped at the first hyphen left
    // `typ-panel-editor` and `typ-panel-tree` a version behind while the other
    // seven moved.
    let manifest = fs::read_to_string(workspace_root().join("Cargo.toml")).expect("root manifest");

    let workspace_version = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .expect("[workspace.package] version")
        .trim()
        .trim_matches('"')
        .to_string();

    let stale: Vec<&str> = manifest
        .lines()
        .filter(|line| line.contains("path = \"crates/"))
        .filter(|line| !line.contains(&format!("version = \"{workspace_version}\"")))
        .collect();

    assert!(
        stale.is_empty(),
        "these path dependencies are not at {workspace_version}:\n  {}",
        stale.join("\n  ")
    );
}
