//! A server process starts, is talked to, and dies when the editor does.
//!
//! The grandchild test is the reason this file exists. rust-analyzer spawns
//! cargo, which spawns rustc; killing only the direct child leaves that subtree
//! running and a core busy until the machine is rebooted. It is the slowest
//! test here and the one worth the most.

use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use lsp_server::{Message, Notification};
use typ_lsp::{Incoming, SpawnError, Transport};

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-lsp-fake-server")
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn start(flags: &[&str]) -> (Transport, Receiver<Incoming>) {
    let (tx, rx) = channel();
    let transport = Transport::spawn(fake(), &args(flags), Path::new("."), tx).expect("it starts");
    (transport, rx)
}

/// The next message, or a failure that says what was waited for.
fn next(rx: &Receiver<Incoming>) -> Message {
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Incoming::Message(m)) => *m,
        Ok(Incoming::Closed) => panic!("the server closed before answering"),
        Err(e) => panic!("nothing arrived: {e}"),
    }
}

fn initialize() -> Message {
    Message::Request(lsp_server::Request {
        id: 1.into(),
        method: "initialize".into(),
        params: serde_json::json!({ "capabilities": {} }),
    })
}

#[test]
fn a_missing_binary_is_a_reason_not_a_panic() {
    // The ordinary case on most machines: nobody has every language server
    // installed, and that must not read as an error.
    let (tx, _rx) = channel::<Incoming>();
    let err = Transport::spawn("definitely-not-a-language-server", &[], Path::new("."), tx)
        .err()
        .expect("a missing binary cannot start");
    assert!(matches!(err, SpawnError::NotFound { .. }), "was: {err:?}");
}

#[test]
fn a_request_written_comes_back_answered() {
    // The claim the whole dependency choice rests on: lsp-server's framing,
    // which is written for a server's own stdio, works over a child's pipes.
    let (transport, rx) = start(&[]);
    transport.send(initialize());
    match next(&rx) {
        Message::Response(response) => {
            assert!(response.response_result.is_ok(), "{response:?}");
        }
        other => panic!("expected a response, got {other:?}"),
    }
}

#[test]
fn a_malformed_frame_does_not_lose_the_connection() {
    // A server that emits one bad frame has not necessarily died, and dropping
    // the connection over it loses everything it would have said afterwards.
    let (transport, rx) = start(&["--garbage"]);
    transport.send(initialize());
    let mut saw_response = false;
    for _ in 0..4 {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Incoming::Message(m)) => {
                if matches!(*m, Message::Response(_)) {
                    saw_response = true;
                    break;
                }
            }
            Ok(Incoming::Closed) => break,
            Err(_) => break,
        }
    }
    assert!(saw_response, "the connection did not survive a bad frame");
}

#[test]
fn a_server_that_exits_reports_it_rather_than_hanging() {
    let (_transport, rx) = start(&["--exit-now"]);
    let mut closed = false;
    for _ in 0..4 {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Incoming::Closed) => {
                closed = true;
                break;
            }
            Ok(Incoming::Message(_)) => {}
            Err(_) => break,
        }
    }
    assert!(closed, "a dead server must be reported, not waited on");
}

#[test]
fn a_server_request_reaches_the_client() {
    // The half clients forget. rust-analyzer sends workspace/configuration,
    // and one that never answers leaves the server waiting forever.
    let (transport, rx) = start(&["--server-request"]);
    transport.send(Message::Notification(Notification {
        method: "initialized".into(),
        params: serde_json::json!({}),
    }));
    match next(&rx) {
        Message::Request(request) => assert_eq!(request.method, "workspace/configuration"),
        other => panic!("expected a request from the server, got {other:?}"),
    }
}

#[test]
fn closing_the_input_lets_the_server_finish_on_its_own() {
    // The polite half of shutdown. A server that sees end of input stops; if
    // this did not work, every quit would go through the kill path and
    // rust-analyzer would never get to write its state.
    let (mut transport, _rx) = start(&[]);
    transport.close_input();
    assert!(
        transport.wait_for_exit(Duration::from_secs(10)),
        "the server did not exit on end of input"
    );
}

#[test]
fn dropping_the_transport_kills_the_child() {
    let (transport, _rx) = start(&["--sleep"]);
    let pid = transport.pid();
    assert!(alive(pid), "the fixture must start something alive");
    drop(transport);
    assert!(gone(pid), "the child outlived the transport");
}

#[test]
fn dropping_the_transport_kills_the_grandchildren() {
    // The one that matters, and the reason this file is slow. A server's own
    // children are the expensive ones — cargo and rustc, not rust-analyzer.
    let (transport, rx) = start(&["--spawn-child"]);
    transport.send(initialize());

    let grandchild = (0..4)
        .find_map(|_| match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Incoming::Message(m)) => match *m {
                Message::Notification(n) if n.method == "fake/grandchild" => n
                    .params
                    .get("pid")
                    .and_then(|p| p.as_u64())
                    .map(|p| p as u32),
                _ => None,
            },
            _ => None,
        })
        .expect("the fake server reported no grandchild");

    assert!(
        alive(grandchild),
        "the fixture must start a live grandchild"
    );
    drop(transport);
    assert!(gone(grandchild), "the grandchild survived the transport");
}

/// Whether a process still exists, waiting a little for it not to.
///
/// Termination is not instant on either platform, so `gone` polls rather than
/// asking once and calling a scheduling delay a bug.
fn gone(pid: u32) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        // A handle can outlive the process, so "it opened" is not enough.
        ok && code == STILL_ACTIVE as u32
    }
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // Signal 0 performs the permission and existence checks and sends nothing.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
