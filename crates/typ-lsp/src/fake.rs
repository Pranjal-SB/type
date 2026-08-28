//! A language server that does as little as possible, and misbehaves on request.
//!
//! **A test double that ships in the library.** It lives here rather than only
//! in `src/bin` because `CARGO_BIN_EXE_` is set only for bin targets of the
//! package under test, so `typ-app`'s tests cannot reach `typ-lsp`'s binary.
//! Both packages declare a three-line bin over this function instead of
//! keeping two copies of a server in sync. Nothing in `typ` reaches it, so
//! `lto = "fat"` drops it from the shipped binary.
//!
//! CI cannot depend on rust-analyzer being installed, and a test that waits on
//! a real indexer is a test nobody runs. This is the only server three
//! platforms will agree on.
//!
//! **The flags are the point.** Anyone can test a server that works. The paths
//! that break clients are the ones where a server offers only UTF-16, answers
//! slowly, sends a request of its own, emits a malformed frame, or dies —
//! and none of those are reachable with a well-behaved double.

use std::io::{BufReader, Write, stdin, stdout};

use lsp_server::{Message, Notification, Request, Response};

struct Flags {
    utf8: bool,
    pull: bool,
    server_request: bool,
    /// Publish diagnostics on `didOpen` and on `didSave`, the way a server
    /// whose checker runs on save does.
    push: bool,
    /// Publish on `didChange` stamped with version 0, which is stale the
    /// moment anything has been typed. The client has to drop it.
    push_stale: bool,
    /// Answer `textDocument/definition` with a sibling file rather than with
    /// the document itself, so a test can tell "jumped within the file" from
    /// "opened another one".
    definition_elsewhere: bool,
    /// Answer `textDocument/definition` with a file that is not there.
    definition_missing: bool,
    /// Answer `textDocument/hover` with a plain string rather than markup.
    hover_plain: bool,
    /// Answer nothing at all to a definition request. Servers do this when
    /// they have not finished indexing.
    no_definition: bool,
    garbage: bool,
    exit_now: bool,
    sleep: bool,
    spawn_child: bool,
    die_after: Option<usize>,
}

impl Flags {
    fn parse() -> Flags {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let has = |name: &str| args.iter().any(|a| a == name);
        Flags {
            utf8: !has("--no-utf8"),
            pull: has("--pull"),
            server_request: has("--server-request"),
            push: has("--push") || has("--push-stale"),
            push_stale: has("--push-stale"),
            definition_elsewhere: has("--definition-elsewhere"),
            definition_missing: has("--definition-missing"),
            hover_plain: has("--hover-plain"),
            no_definition: has("--no-definition"),
            garbage: has("--garbage"),
            exit_now: has("--exit-now"),
            sleep: has("--sleep"),
            spawn_child: has("--spawn-child"),
            die_after: args
                .iter()
                .find_map(|a| a.strip_prefix("--die-after="))
                .and_then(|n| n.parse().ok()),
        }
    }
}

fn capabilities(flags: &Flags) -> serde_json::Value {
    let mut caps = serde_json::json!({
        "textDocumentSync": 1,
        "definitionProvider": true,
        "hoverProvider": true,
    });
    if flags.pull {
        caps["diagnosticProvider"] = serde_json::json!({
            "identifier": "fake-native",
            "interFileDependencies": false,
            "workspaceDiagnostics": false,
        });
    }
    caps
}

/// A URI naming `name` in the same directory as `uri`.
fn sibling(uri: &str, name: &str) -> String {
    match uri.rfind('/') {
        Some(cut) => format!("{}/{name}", &uri[..cut]),
        None => uri.to_string(),
    }
}

/// Send `textDocument/publishDiagnostics` for one document.
///
/// Each entry is a line, an LSP severity and a message. The range covers the
/// first three characters of the line, which is enough for a renderer to have
/// something to underline.
fn publish(out: &mut impl Write, uri: &str, version: i64, items: &[(u32, i64, &str)]) {
    let diagnostics: Vec<serde_json::Value> = items
        .iter()
        .map(|(line, severity, message)| {
            serde_json::json!({
                "range": {
                    "start": { "line": line, "character": 0 },
                    "end":   { "line": line, "character": 3 },
                },
                "severity": severity,
                "message": message,
                "source": "fake",
            })
        })
        .collect();
    let _ = Message::Notification(Notification {
        method: "textDocument/publishDiagnostics".into(),
        params: serde_json::json!({
            "uri": uri,
            "version": version,
            "diagnostics": diagnostics,
        }),
    })
    .write(out);
    let _ = out.flush();
}

/// Read frames from stdin and answer them until told to exit.
pub fn run() {
    let flags = Flags::parse();

    if flags.exit_now {
        return;
    }

    if flags.sleep {
        // Block forever. Used as the grandchild in the process-tree test: the
        // point is that it outlives its parent unless something kills the whole
        // tree, so it must not exit on its own.
        std::thread::park();
        return;
    }

    let mut out = stdout();

    if flags.spawn_child {
        // Stand in for rust-analyzer spawning cargo, which spawns rustc.
        // Spawning ourselves keeps this portable — there is no `sleep` on
        // Windows and no `ping -n` worth relying on.
        let me = std::env::current_exe().expect("our own path");
        let child = std::process::Command::new(me)
            .arg("--sleep")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("a grandchild");
        let _ = Message::Notification(Notification {
            method: "fake/grandchild".into(),
            params: serde_json::json!({ "pid": child.id() }),
        })
        .write(&mut out);
        let _ = out.flush();
        // Deliberately leaked: dropping a Child does not wait, and we want the
        // grandchild to outlive this process unless the job object takes it.
        std::mem::forget(child);
    }

    if flags.garbage {
        // Not a frame. A client that treats this as fatal loses a connection
        // it could have kept.
        let _ = out.write_all(b"Content-Length: 9\r\n\r\nnot-json!");
        let _ = out.flush();
    }

    let mut input = BufReader::new(stdin());
    let mut seen = 0usize;
    let mut open_uri = String::new();
    let mut version = 0i64;

    while let Ok(Some(message)) = Message::read(&mut input) {
        seen += 1;
        if flags.die_after == Some(seen) {
            return;
        }

        match message {
            Message::Request(Request { id, method, params }) => {
                let result = match method.as_str() {
                    // `clientSaw` is not LSP. It echoes the initialize params
                    // straight back so a test can assert what the client sent
                    // — a typo in `positionEncodings` would otherwise be
                    // invisible, falling back to UTF-16 and still working.
                    "initialize" => serde_json::json!({
                        "capabilities": capabilities(&flags),
                        "positionEncoding": if flags.utf8 { "utf-8" } else { "utf-16" },
                        "clientSaw": params,
                    }),
                    "shutdown" => serde_json::Value::Null,
                    // The URI it was *asked* about, so a client can be tested
                    // for jumping inside one file. A fixed made-up path would
                    // only ever exercise the failure branch.
                    "textDocument/definition" if flags.no_definition => serde_json::Value::Null,
                    "textDocument/definition" => {
                        let asked = params
                            .pointer("/textDocument/uri")
                            .and_then(|u| u.as_str())
                            .unwrap_or("file:///fake/target.rs");
                        let uri = if flags.definition_missing {
                            sibling(asked, "not-there.rs")
                        } else if flags.definition_elsewhere {
                            sibling(asked, "target.rs")
                        } else {
                            asked.to_string()
                        };
                        serde_json::json!({
                            "uri": uri,
                            "range": {
                                "start": { "line": 4, "character": 2 },
                                "end":   { "line": 4, "character": 5 },
                            },
                        })
                    }
                    "textDocument/hover" if flags.hover_plain => serde_json::json!({
                        "contents": "plain words",
                    }),
                    "textDocument/hover" => serde_json::json!({
                        "contents": {
                            "kind": "markdown",
                            "value": "```rust\nfn fake()\n```\n\nDoes **nothing**.",
                        },
                    }),
                    _ => serde_json::Value::Null,
                };
                let _ = Message::Response(Response {
                    id,
                    response_result: Ok(result),
                })
                .write(&mut out);
                let _ = out.flush();
            }
            Message::Notification(Notification { method, params }) => {
                if method == "exit" {
                    return;
                }
                if let Some(doc) = params.get("textDocument") {
                    if let Some(uri) = doc.get("uri").and_then(|u| u.as_str()) {
                        open_uri = uri.to_string();
                    }
                    if let Some(v) = doc.get("version").and_then(|v| v.as_i64()) {
                        version = v;
                    }
                }
                match method.as_str() {
                    "textDocument/didOpen" if flags.push => {
                        publish(&mut out, &open_uri, version, &[(5, 1, "fake: on open")]);
                    }
                    "textDocument/didSave" if flags.push => {
                        publish(
                            &mut out,
                            &open_uri,
                            version,
                            &[(5, 1, "fake: on open"), (1, 2, "fake: on save")],
                        );
                    }
                    "textDocument/didChange" if flags.push_stale => {
                        // Deliberately the version the document had when it was
                        // opened. A client that does not compare versions shows
                        // this and loses what it already had.
                        publish(&mut out, &open_uri, 0, &[(9, 1, "fake: stale")]);
                    }
                    _ => {}
                }
                if method == "initialized" && flags.server_request {
                    // The half clients forget. rust-analyzer really does this,
                    // and a client that never answers leaves it waiting.
                    let _ = Message::Request(Request {
                        id: 1.into(),
                        method: "workspace/configuration".into(),
                        params: serde_json::json!({ "items": [] }),
                    })
                    .write(&mut out);
                    let _ = out.flush();
                }
            }
            Message::Response(_) => {}
        }
    }
}
