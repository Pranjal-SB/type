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
