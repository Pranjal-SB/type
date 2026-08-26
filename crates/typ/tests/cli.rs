use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typ")
}

#[test]
fn missing_path_exits_nonzero_with_a_message_on_stderr() {
    let out = Command::new(bin())
        .arg("definitely/does/not/exist.rs")
        .output()
        .expect("binary runs");
    assert!(!out.status.success(), "expected a non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does not exist"), "stderr was: {stderr}");
}

#[test]
fn a_bad_path_after_a_good_one_still_exits_nonzero() {
    // `typ a.rs nope/deep.rs` used to succeed by ignoring the second path
    // entirely. Now that extra paths open as tabs they get the same check the
    // first one gets — a missing parent directory fails before the alternate
    // screen is entered, while stderr is still visible. Arg 1 and arg 2
    // disagreeing about what is a valid path is worse than either answer.
    let dir = std::env::temp_dir().join("typ-cli-multi");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture");
    let good = dir.join("good.rs");
    std::fs::write(&good, "fn good() {}\n").expect("fixture");

    let out = Command::new(bin())
        .arg(&good)
        .arg(dir.join("nope").join("deep.rs"))
        .output()
        .expect("binary runs");

    assert!(!out.status.success(), "expected a non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does not exist"), "stderr was: {stderr}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_flag_prints_and_exits_zero() {
    let out = Command::new(bin())
        .arg("--version")
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("typ"));
}

#[test]
fn help_flag_names_the_binary() {
    let out = Command::new(bin())
        .arg("--help")
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("typ"));
}
