//! Paths to `file://` URIs and back, on every platform CI runs.
//!
//! Decoding one of these wrong opens a different file and says nothing, which
//! is why the non-ASCII and space cases are here rather than assumed.

use std::path::PathBuf;

use typ_lsp::{path_to_uri, uri_to_path};

fn abs(parts: &[&str]) -> PathBuf {
    let mut p = std::env::current_dir().expect("a working directory");
    for part in parts {
        p.push(part);
    }
    p
}

#[test]
fn a_plain_path_round_trips() {
    let path = abs(&["src", "main.rs"]);
    let uri = path_to_uri(&path).expect("an absolute path has a uri");
    assert_eq!(uri_to_path(&uri).as_deref(), Some(path.as_path()));
}

#[test]
fn a_path_with_a_space_round_trips() {
    let path = abs(&["my project", "a.rs"]);
    let uri = path_to_uri(&path).expect("uri");
    assert!(
        uri.as_str().contains("%20"),
        "a space must be percent-encoded: {}",
        uri.as_str()
    );
    assert_eq!(uri_to_path(&uri).as_deref(), Some(path.as_path()));
}

#[test]
fn a_non_ascii_path_round_trips() {
    // The one that matters. Getting this wrong opens a different file.
    let path = abs(&["日本語", "café.rs"]);
    let uri = path_to_uri(&path).expect("uri");
    assert_eq!(uri_to_path(&uri).as_deref(), Some(path.as_path()));
}

#[test]
fn a_relative_path_is_refused_rather_than_guessed() {
    assert!(path_to_uri(&PathBuf::from("relative/a.rs")).is_none());
}

#[test]
fn a_uri_that_is_not_a_file_has_no_path() {
    let uri: lsp_types::Uri = "https://example.com/a.rs".parse().expect("parses");
    assert!(uri_to_path(&uri).is_none());
}

#[cfg(windows)]
#[test]
fn a_windows_path_gets_three_slashes_and_a_drive_letter() {
    let uri = path_to_uri(&PathBuf::from(r"C:\Users\a\b.rs")).expect("uri");
    assert!(
        uri.as_str().starts_with("file:///C:/"),
        "was: {}",
        uri.as_str()
    );
}

#[cfg(unix)]
#[test]
fn a_unix_path_keeps_its_leading_slash() {
    let uri = path_to_uri(&PathBuf::from("/home/a/b.rs")).expect("uri");
    assert_eq!(uri.as_str(), "file:///home/a/b.rs");
}
