use std::path::{Component, Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Which paths under a root the picker's walk would skip, asked one path at a
/// time: what the workspace watcher filters its events through.
///
/// The walk's rules, as far as one path can be judged without walking to it:
/// a hidden component (which is what keeps `.git/` out) and the root's
/// `.gitignore` and `.ignore`, honoured whether or not git is present, as
/// `walk` does with `require_git(false)`.
///
/// ponytail: root ignore files only. A nested `.gitignore` or the global one
/// is not read, so a change under a directory only they exclude is still
/// reported. That costs the receiver one lookup that finds no open tab; read
/// them here when a profile says the events cost something.
#[derive(Clone)]
pub struct Ignore {
    root: PathBuf,
    rules: Gitignore,
}

impl Ignore {
    /// Read the root's ignore files. A missing or unparsable one ignores
    /// nothing, rather than failing: this crate cannot log, and over-reporting
    /// a change is harmless where missing one is not.
    pub fn new(root: &Path) -> Self {
        let mut builder = GitignoreBuilder::new(root);
        for name in [".gitignore", ".ignore"] {
            let file = root.join(name);
            if file.is_file() {
                let _ = builder.add(file);
            }
        }
        Ignore {
            root: root.to_path_buf(),
            rules: builder.build().unwrap_or_else(|_| Gitignore::empty()),
        }
    }

    /// Whether the walk would skip `path`. A path outside the root is not this
    /// workspace's to judge, and is never ignored.
    pub fn ignores(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let hidden = relative.components().any(|part| {
            matches!(part, Component::Normal(name) if name.to_string_lossy().starts_with('.'))
        });
        hidden
            || self
                .rules
                .matched_path_or_any_parents(relative, path.is_dir())
                .is_ignore()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("typ-find-ignore").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
        dir
    }

    #[test]
    fn the_walks_rules_say_what_a_change_may_be_ignored_for() {
        let dir = root("rules");
        let ignore = Ignore::new(&dir);
        assert!(ignore.ignores(&dir.join("target/debug/x.rs")));
        assert!(ignore.ignores(&dir.join(".git/HEAD")));
        assert!(ignore.ignores(&dir.join("src/run.log")));
        assert!(!ignore.ignores(&dir.join("src/main.rs")));
        assert!(!ignore.ignores(Path::new("/elsewhere/target/x.rs")));
    }
}
