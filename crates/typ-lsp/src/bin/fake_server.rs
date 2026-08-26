//! A language server that does as little as possible, and misbehaves on request.
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

fn main() {
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

    while let Ok(Some(message)) = Message::read(&mut input) {
        seen += 1;
        if flags.die_after == Some(seen) {
            return;
        }

        match message {
            Message::Request(Request { id, method, .. }) => {
                let result = match method.as_str() {
                    "initialize" => serde_json::json!({
                        "capabilities": capabilities(&flags),
                        "positionEncoding": if flags.utf8 { "utf-8" } else { "utf-16" },
                    }),
                    "shutdown" => serde_json::Value::Null,
                    "textDocument/definition" => serde_json::json!({
                        "uri": "file:///fake/target.rs",
                        "range": {
                            "start": { "line": 4, "character": 0 },
                            "end":   { "line": 4, "character": 3 },
                        },
                    }),
                    "textDocument/hover" => serde_json::json!({
                        "contents": { "kind": "markdown", "value": "`fn fake()`" },
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
            Message::Notification(Notification { method, .. }) => {
                if method == "exit" {
                    return;
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
