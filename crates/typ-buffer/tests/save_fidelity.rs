//! Saving must not vandalise what it did not write.
//!
//! Deferred from M2.1 and true since: `save` wrote LF into every file, replaced
//! symlinks with regular files, and dropped POSIX mode bits. Each is a small
//! silent act of damage to somebody else's file, and each is invisible until it
//! shows up as a whole-file diff or a script that stopped being executable.

use std::path::PathBuf;

use typ_buffer::{LineEnding, Position, TextBuffer};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-save-fidelity").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_crlf_file_is_still_crlf_after_an_edit_and_a_save() {
    let dir = dir("crlf-roundtrip");
    let path = dir.join("windows.txt");
    std::fs::write(&path, "one\r\ntwo\r\nthree\r\n").unwrap();

    let mut buffer = TextBuffer::from_path(&path).unwrap();
    assert_eq!(buffer.line_ending(), LineEnding::Crlf);
    buffer.insert_char(Position { line: 0, col: 0 }, 'x');
    buffer.save().unwrap();

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, "xone\r\ntwo\r\nthree\r\n");
}

#[test]
fn a_newline_typed_into_a_crlf_file_is_a_crlf() {
    let dir = dir("crlf-split");
    let path = dir.join("windows.txt");
    std::fs::write(&path, "one\r\ntwo\r\n").unwrap();

    let mut buffer = TextBuffer::from_path(&path).unwrap();
    // Split "one" after the first grapheme, the way Enter does.
    buffer.insert_char(Position { line: 0, col: 1 }, '\n');
    buffer.save().unwrap();

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        on_disk, "o\r\nne\r\ntwo\r\n",
        "an edit introduced an LF into a CRLF file"
    );
}

#[test]
fn an_lf_file_stays_lf() {
    let dir = dir("lf-roundtrip");
    let path = dir.join("unix.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();

    let mut buffer = TextBuffer::from_path(&path).unwrap();
    buffer.insert_char(Position { line: 0, col: 0 }, 'x');
    buffer.save().unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "xone\ntwo\n");
}

/// Task 2's own-save suppression compares the file against the buffer. With
/// line endings converted on the way out, that comparison has to convert too,
/// or every save of a CRLF file reports itself as an external change.
#[test]
fn a_saved_crlf_file_matches_what_the_buffer_holds() {
    let dir = dir("crlf-matches-disk");
    let path = dir.join("windows.txt");
    std::fs::write(&path, "one\r\ntwo\r\n").unwrap();

    let mut buffer = TextBuffer::from_path(&path).unwrap();
    buffer.insert_char(Position { line: 0, col: 0 }, 'x');
    buffer.save().unwrap();

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, buffer.text_as_saved());
}

#[cfg(unix)]
#[test]
fn saving_through_a_symlink_keeps_the_link() {
    use std::os::unix::fs::symlink;

    let dir = dir("symlink");
    let real = dir.join("real.txt");
    let link = dir.join("link.txt");
    std::fs::write(&real, "hello\n").unwrap();
    symlink(&real, &link).unwrap();

    let mut buffer = TextBuffer::from_path(&link).unwrap();
    buffer.insert_char(Position { line: 0, col: 0 }, 'x');
    buffer.save().unwrap();

    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the symlink was replaced by a regular file; dotfile repos are made of these"
    );
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "xhello\n");
}

#[cfg(unix)]
#[test]
fn an_executable_file_is_still_executable_after_a_save() {
    use std::os::unix::fs::PermissionsExt;

    let dir = dir("mode-bits");
    let path = dir.join("script.sh");
    std::fs::write(&path, "#!/bin/sh\necho hi\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut buffer = TextBuffer::from_path(&path).unwrap();
    buffer.insert_char(Position { line: 1, col: 0 }, '#');
    buffer.save().unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o755,
        "a script stopped being executable because it was edited"
    );
}

#[cfg(unix)]
#[test]
fn a_private_file_does_not_become_world_readable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = dir("mode-private");
    let path = dir.join("secrets.env");
    std::fs::write(&path, "TOKEN=1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let mut buffer = TextBuffer::from_path(&path).unwrap();
    buffer.insert_char(Position { line: 0, col: 0 }, '#');
    buffer.save().unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "an edit widened the permissions on a secret");
}

#[test]
fn a_new_file_is_created_with_the_default_line_ending() {
    let dir = dir("new-file");
    let path = dir.join("fresh.txt");

    let mut buffer = TextBuffer::new_at(&path);
    buffer.insert_char(Position { line: 0, col: 0 }, 'a');
    buffer.save().unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a");
}
