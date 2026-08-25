use std::fs;
use std::path::{Path, PathBuf};

/// A throwaway tree under the OS temp dir, removed on drop.
///
/// Not `tempfile`: the workspace has no equivalent and this is twenty lines
/// against a dependency. The directory name carries the test's own name so a
/// leaked one says which test leaked it.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-find-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture root");
        Fixture(dir)
    }

    fn file(&self, rel: &str, contents: &str) -> &Self {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().expect("has a parent")).expect("fixture dirs");
        fs::write(path, contents).expect("fixture file");
        self
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_walk_finds_files_and_not_directories() {
    let fixture = Fixture::new("files-not-dirs");
    fixture
        .file("src/main.rs", "fn main() {}")
        .file("src/lib.rs", "")
        .file("README.md", "# hi");

    let mut found = typ_find::walk(fixture.path());
    found.sort();

    assert_eq!(found, vec!["README.md", "src/lib.rs", "src/main.rs"]);
}

#[test]
fn gitignored_paths_are_absent() {
    // The reason `ignore` is a dependency rather than a `read_dir` loop. A
    // picker that offers 40,000 files out of target/ is a picker nobody opens
    // twice.
    let fixture = Fixture::new("gitignore");
    fixture
        .file(".gitignore", "target/\n*.log\n")
        .file("src/main.rs", "")
        .file("target/debug/huge", "")
        .file("noisy.log", "");

    let found = typ_find::walk(fixture.path());

    assert!(found.contains(&"src/main.rs".to_string()), "got {found:?}");
    assert!(
        !found.iter().any(|p| p.starts_with("target/")),
        "target/ was not ignored: {found:?}"
    );
    assert!(
        !found.iter().any(|p| p.ends_with(".log")),
        "*.log was not ignored: {found:?}"
    );
}

#[test]
fn dotfiles_are_absent_by_default() {
    // `.git/` in particular: it is thousands of files nobody wants to open, and
    // it is not in anyone's `.gitignore` because it does not need to be.
    let fixture = Fixture::new("hidden");
    fixture
        .file("visible.rs", "")
        .file(".git/config", "")
        .file(".env", "secret");

    let found = typ_find::walk(fixture.path());

    assert_eq!(found, vec!["visible.rs".to_string()], "got {found:?}");
}

#[test]
fn separators_are_forward_slashes_on_every_platform() {
    // The candidate string is what gets scored, and `crates\typ-core` scores
    // differently from `crates/typ-core` under a path-aware matcher. A picker
    // that ranks differently on Windows is a picker with two behaviours.
    let fixture = Fixture::new("separators");
    fixture.file("a/b/c/deep.rs", "");

    let found = typ_find::walk(fixture.path());

    assert_eq!(found, vec!["a/b/c/deep.rs".to_string()]);
    assert!(!found[0].contains('\\'), "backslash leaked: {found:?}");
}

#[test]
fn paths_are_relative_to_the_root() {
    // Absolute paths would put the user's home directory in every candidate,
    // which both wastes the width and gives the matcher a long identical prefix
    // to score through.
    let fixture = Fixture::new("relative");
    fixture.file("src/main.rs", "");

    let found = typ_find::walk(fixture.path());

    assert_eq!(found, vec!["src/main.rs".to_string()]);
}

#[test]
fn the_root_itself_is_not_a_candidate() {
    let fixture = Fixture::new("root");
    fixture.file("only.rs", "");

    let found = typ_find::walk(fixture.path());

    assert!(!found.iter().any(|p| p.is_empty()), "got {found:?}");
    assert!(!found.iter().any(|p| p == "."), "got {found:?}");
}

#[test]
fn a_root_that_does_not_exist_is_an_empty_list() {
    // Not a panic and not an error: `typ-find` sits below `typ-app` and cannot
    // log, so the only honest thing it can return is "no candidates". The app
    // opening a picker on a deleted directory shows an empty list, which is
    // what is true.
    let found = typ_find::walk(Path::new("definitely/does/not/exist"));
    assert!(found.is_empty(), "got {found:?}");
}
