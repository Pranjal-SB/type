//! The handshake: what TYPE tells a server, and what it does with the answer.

use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use typ_lsp::{Client, Encoding, Incoming, LspEvent};

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-lsp-fake-server")
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn start(flags: &[&str]) -> (Client, Receiver<Incoming>) {
    let (tx, rx) = channel();
    let client = Client::start(fake(), &args(flags), Path::new("."), tx).expect("it starts");
    (client, rx)
}

/// Feed messages to the client until it says it is initialized.
///
/// The client owns no thread — the app drives it — so a test has to drive it
/// the same way. That is the point of the design, not a limitation of it.
fn pump_until_initialized(client: &mut Client, rx: &Receiver<Incoming>) -> Vec<LspEvent> {
    let mut surfaced = Vec::new();
    for _ in 0..8 {
        if client.is_initialized() {
            return surfaced;
        }
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(incoming) => surfaced.extend(client.handle(incoming)),
            Err(e) => panic!("nothing arrived while initializing: {e}"),
        }
    }
    assert!(client.is_initialized(), "the handshake never completed");
    surfaced
}

#[test]
fn the_handshake_completes_and_records_capabilities() {
    let (mut client, rx) = start(&[]);
    assert!(
        !client.is_initialized(),
        "it cannot be done before it starts"
    );
    pump_until_initialized(&mut client, &rx);
    assert!(client.supports("definitionProvider"));
    assert!(client.supports("hoverProvider"));
}

#[test]
fn the_initialize_response_is_not_surfaced_to_the_app() {
    // Capability negotiation is the client's business. An app that had to know
    // which response was the handshake would be doing the client's job.
    let (mut client, rx) = start(&[]);
    let surfaced = pump_until_initialized(&mut client, &rx);
    assert!(surfaced.is_empty(), "leaked: {surfaced:?}");
}

#[test]
fn utf8_is_taken_when_the_server_offers_it() {
    let (mut client, rx) = start(&[]);
    pump_until_initialized(&mut client, &rx);
    assert_eq!(client.encoding(), Encoding::Utf8);
}

#[test]
fn utf16_is_the_fallback_when_it_is_all_that_is_offered() {
    // Most servers in the field. This is the common path, not the edge.
    let (mut client, rx) = start(&["--no-utf8"]);
    pump_until_initialized(&mut client, &rx);
    assert_eq!(client.encoding(), Encoding::Utf16);
}

/// The initialize params the server actually received.
///
/// The fake server echoes them back inside its result under `clientSaw`, which
/// is not LSP and exists only so a test can assert what was sent. Nothing about
/// the client's own capabilities is observable otherwise, and a typo in one of
/// them is silent — the server ignores an unknown key and everything still
/// works, slightly worse, for no stated reason.
fn what_the_server_was_told(flags: &[&str]) -> serde_json::Value {
    let (mut client, rx) = start(flags);
    let mut echoed = None;
    for _ in 0..8 {
        if client.is_initialized() {
            break;
        }
        let Ok(incoming) = rx.recv_timeout(Duration::from_secs(10)) else {
            break;
        };
        if let Incoming::Message(message) = &incoming
            && let lsp_server::Message::Response(response) = &**message
            && let Ok(result) = &response.response_result
        {
            echoed = result.get("clientSaw").cloned();
        }
        client.handle(incoming);
    }
    echoed.expect("the fake server echoes what it was sent")
}

#[test]
fn the_client_offers_every_encoding_in_preference_order() {
    // UTF-32 first because it is ropey's char index unchanged, UTF-16 last
    // because it is mandatory rather than good.
    let told = what_the_server_was_told(&[]);
    let offered = told
        .pointer("/capabilities/general/positionEncodings")
        .and_then(|v| v.as_array())
        .expect("positionEncodings was sent");
    let names: Vec<&str> = offered.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(names, ["utf-32", "utf-8", "utf-16"]);
}

#[test]
fn dynamic_registration_for_watched_files_is_declined() {
    // Deliberate, not an omission. A server can only register for
    // `workspace/didChangeWatchedFiles` through `client/registerCapability`;
    // claiming support and then not handling the registration is the failure
    // mode. False makes rust-analyzer watch files itself, which works. The day
    // someone flips this, they have to build the other half too.
    let told = what_the_server_was_told(&[]);
    assert_eq!(
        told.pointer("/capabilities/workspace/didChangeWatchedFiles/dynamicRegistration"),
        Some(&serde_json::Value::Bool(false)),
    );
}

#[test]
fn a_request_from_the_server_is_surfaced_rather_than_swallowed() {
    let (mut client, rx) = start(&["--server-request"]);
    pump_until_initialized(&mut client, &rx);

    let event = (0..4)
        .find_map(|_| {
            let incoming = rx.recv_timeout(Duration::from_secs(10)).ok()?;
            client.handle(incoming)
        })
        .expect("the server's request never arrived");

    match event {
        LspEvent::ServerRequest { method, .. } => {
            assert_eq!(method, "workspace/configuration");
        }
        other => panic!("expected a server request, got {other:?}"),
    }
}

#[test]
fn a_dead_server_surfaces_as_exited_exactly_once() {
    let (mut client, rx) = start(&["--exit-now"]);
    let mut exits = 0;
    while let Ok(incoming) = rx.recv_timeout(Duration::from_secs(5)) {
        if matches!(client.handle(incoming), Some(LspEvent::Exited)) {
            exits += 1;
        }
    }
    assert_eq!(exits, 1, "Exited must arrive once and last");
}

#[test]
fn a_server_that_never_initializes_supports_nothing() {
    // The degradation path. Every capability check answers no, so every
    // feature declines to ask, and the editor is the editor it already was.
    let (client, _rx) = start(&["--exit-now"]);
    assert!(!client.supports("definitionProvider"));
    assert!(!client.is_initialized());
    assert_eq!(client.encoding(), Encoding::Utf16);
}

#[test]
fn shutdown_lets_the_server_stop_on_its_own() {
    let (mut client, rx) = start(&[]);
    pump_until_initialized(&mut client, &rx);
    client.shutdown(Duration::from_secs(10));
}
