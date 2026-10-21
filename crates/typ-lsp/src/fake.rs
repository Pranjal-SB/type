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
    /// Publish on `didOpen` a payload one field off the protocol's shape, so the
    /// client's parse of it fails.
    push_malformed: bool,
    /// Publish on `didOpen` against the URI lowercased: the same file on a
    /// case-insensitive filesystem, and a different `PathBuf`.
    push_respelled: bool,
    /// Publish on `didOpen` and `didChange` one diagnostic whose message is
    /// the document text the notification carried.
    echo: bool,
    /// Answer `textDocument/definition` with a sibling file rather than with
    /// the document itself, so a test can tell "jumped within the file" from
    /// "opened another one".
    definition_elsewhere: bool,
    /// Answer `textDocument/definition` with a file that is not there.
    definition_missing: bool,
    /// Answer `textDocument/hover` with a plain string rather than markup.
    hover_plain: bool,
    /// Answer `textDocument/hover` with `null`. Legal, and the ordinary answer
    /// while a server is still indexing.
    hover_empty: bool,
    /// Answer `textDocument/hover` with a real error rather than a result.
    hover_error: bool,
    /// Answer the first N `textDocument/hover` requests with `ContentModified`
    /// before answering properly. What rust-analyzer does whenever a salsa
    /// cancellation lands mid-request and it has no retry of its own: the code
    /// is not a refusal, it is "ask me again".
    content_modified: usize,
    /// Answer `initialize` with an error, and then stay up. A server that
    /// refuses the workspace it was started in.
    refuse_initialize: bool,
    /// Answer nothing at all to a definition request. Servers do this when
    /// they have not finished indexing.
    no_definition: bool,
    /// Report indexing progress after `initialized`, the way a server with
    /// real work to do at startup does.
    progress: bool,
    /// Ask the client to create the progress token first, which is a *request*
    /// and hangs the server if nobody answers it.
    progress_create: bool,
    /// Leave `completionProvider` out of the capabilities.
    no_completion: bool,
    /// Hold each completion answer until the next `didChange` arrives, so it
    /// describes a document the client has already moved past.
    complete_late: bool,
    /// Leave `signatureHelpProvider` out of the capabilities.
    no_signature: bool,
    /// Leave `codeActionProvider` out of the capabilities.
    no_code_action: bool,
    /// Offer `renameProvider` without `prepareProvider`.
    no_prepare_rename: bool,
    /// Answer `textDocument/prepareRename` with an error saying why not.
    refuse_rename: bool,
    /// Offer `日本語` as well, with a `textEdit` over the word before the
    /// position, counted in the encoding this server speaks.
    complete_edit: bool,
    garbage: bool,
    exit_now: bool,
    sleep: bool,
    spawn_child: bool,
    close_stdin: bool,
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
            push_malformed: has("--push-malformed"),
            push_respelled: has("--push-respelled"),
            echo: has("--echo"),
            definition_elsewhere: has("--definition-elsewhere"),
            definition_missing: has("--definition-missing"),
            hover_plain: has("--hover-plain"),
            hover_empty: has("--hover-empty"),
            hover_error: has("--hover-error"),
            refuse_initialize: has("--refuse-initialize"),
            content_modified: args
                .iter()
                .find_map(|a| a.strip_prefix("--content-modified="))
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
            no_definition: has("--no-definition"),
            progress: has("--progress") || has("--progress-create"),
            progress_create: has("--progress-create"),
            no_completion: has("--no-completion"),
            complete_late: has("--complete-late"),
            complete_edit: has("--complete-edit"),
            no_signature: has("--no-signature"),
            no_code_action: has("--no-code-action"),
            no_prepare_rename: has("--no-prepare-rename"),
            refuse_rename: has("--refuse-rename"),
            garbage: has("--garbage"),
            exit_now: has("--exit-now"),
            sleep: has("--sleep"),
            spawn_child: has("--spawn-child"),
            close_stdin: has("--close-stdin"),
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
    if !flags.no_completion {
        caps["completionProvider"] = serde_json::json!({ "triggerCharacters": ["."] });
    }
    if !flags.no_signature {
        caps["signatureHelpProvider"] = serde_json::json!({ "triggerCharacters": ["(", ","] });
    }
    if !flags.no_code_action {
        caps["codeActionProvider"] = serde_json::json!(true);
        caps["executeCommandProvider"] = serde_json::json!({ "commands": [FIX_COMMAND] });
    }
    caps["renameProvider"] = match flags.no_prepare_rename {
        true => serde_json::json!(true),
        false => serde_json::json!({ "prepareProvider": true }),
    };
    if flags.pull {
        caps["diagnosticProvider"] = serde_json::json!({
            "identifier": "fake-native",
            "interFileDependencies": false,
            "workspaceDiagnostics": false,
        });
    }
    caps
}

/// Send one `$/progress` notification.
fn progress(out: &mut impl Write, token: &str, value: serde_json::Value) {
    let _ = Message::Notification(Notification {
        method: "$/progress".into(),
        params: serde_json::json!({ "token": token, "value": value }),
    })
    .write(out);
    let _ = out.flush();
}

/// The id the fake gives `window/workDoneProgress/create`.
const CREATE_ID: i32 = 7;

/// Two tokens' worth of progress, which is the ordinary rust-analyzer shape
/// rather than the strange one.
fn progress_burst(out: &mut impl Write) {
    progress(
        out,
        "indexing",
        serde_json::json!({ "kind": "begin", "title": "Indexing" }),
    );
    progress(
        out,
        "indexing",
        serde_json::json!({ "kind": "report", "percentage": 40 }),
    );
    progress(
        out,
        "fetching",
        serde_json::json!({ "kind": "begin", "title": "Fetching" }),
    );
}

/// What `textDocument/completion` answers: two functions, one with docs, and
/// under `--complete-edit` a CJK word placed by a `textEdit`.
fn completion(flags: &Flags, text: &str, params: &serde_json::Value) -> serde_json::Value {
    let mut items = vec![
        serde_json::json!({ "label": "foo", "kind": 3, "detail": "fn foo()", "documentation": "Does foo." }),
        serde_json::json!({ "label": "format", "kind": 3, "detail": "fn format(s: &str)" }),
    ];
    if flags.complete_edit {
        let line = params
            .pointer("/position/line")
            .and_then(|l| l.as_u64())
            .unwrap_or(0);
        let at = params
            .pointer("/position/character")
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        let start = word_start(text, line as usize, at as usize, flags.utf8);
        items.push(serde_json::json!({
            "label": "日本語",
            "kind": 6,
            "textEdit": {
                "range": {
                    "start": { "line": line, "character": start },
                    "end": { "line": line, "character": at },
                },
                "newText": "日本語",
            },
        }));
    }
    serde_json::json!({ "isIncomplete": false, "items": items })
}

/// The command the fake's second code action runs.
const FIX_COMMAND: &str = "fake.fix";

/// The id the fake gives `workspace/applyEdit`.
const APPLY_ID: i32 = 8;

/// The line the fake offers code actions on.
const FIX_LINE: u64 = 2;

/// An edit replacing the first three characters of `FIX_LINE` with `text`.
fn fix_edit(uri: &str, text: &str) -> serde_json::Value {
    serde_json::json!({ "changes": { uri: [{
        "range": {
            "start": { "line": FIX_LINE, "character": 0 },
            "end": { "line": FIX_LINE, "character": 3 },
        },
        "newText": text,
    }]}})
}

/// What `textDocument/codeAction` answers: on `FIX_LINE`, a quick fix with an
/// edit and a bare command; anywhere else, nothing.
fn code_actions(params: &serde_json::Value) -> serde_json::Value {
    let line = params.pointer("/range/start/line").and_then(|l| l.as_u64());
    if line != Some(FIX_LINE) {
        return serde_json::json!([]);
    }
    let uri = params
        .pointer("/textDocument/uri")
        .and_then(|u| u.as_str())
        .unwrap_or_default();
    serde_json::json!([
        { "title": "Replace with fix", "kind": "quickfix", "edit": fix_edit(uri, "fix") },
        { "title": "Run the fake fix", "command": FIX_COMMAND, "arguments": [uri] },
    ])
}

/// The word at `character` on `line` of `text`, as its char range and text.
/// Chars stand in for the encoding, which is exact for the ASCII the rename
/// tests use.
fn word_at(text: &str, line: u64, character: u64) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = text.split('\n').nth(line as usize)?.chars().collect();
    let at = (character as usize).min(chars.len());
    let is_word = |c: &char| c.is_alphanumeric() || *c == '_';
    let start = at - chars[..at].iter().rev().take_while(|c| is_word(c)).count();
    let end = at + chars[at..].iter().take_while(|c| is_word(c)).count();
    (start < end).then(|| (start, end, chars[start..end].iter().collect()))
}

/// Every whole-word `word` in `text`, as edits to `new_name`.
fn occurrences(text: &str, word: &str, new_name: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (line, row) in text.split('\n').enumerate() {
        let chars: Vec<char> = row.chars().collect();
        let width = word.chars().count();
        for start in 0..chars.len() {
            let end = start + width;
            let is_word = |i: usize| {
                chars
                    .get(i)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            };
            let here: String = chars[start..end.min(chars.len())].iter().collect();
            if here == word && !(start > 0 && is_word(start - 1)) && !is_word(end) {
                out.push(serde_json::json!({
                    "range": {
                        "start": { "line": line, "character": start },
                        "end": { "line": line, "character": end },
                    },
                    "newText": new_name,
                }));
            }
        }
    }
    out
}

/// What `textDocument/prepareRename` answers: the word at the position and
/// its range, or null off a word.
fn prepare_rename(text: &str, params: &serde_json::Value) -> serde_json::Value {
    let line = params
        .pointer("/position/line")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let at = params
        .pointer("/position/character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let Some((start, end, word)) = word_at(text, line, at) else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "range": {
            "start": { "line": line, "character": start },
            "end": { "line": line, "character": end },
        },
        "placeholder": word,
    })
}

/// What `textDocument/rename` answers: the word at the position renamed in
/// the document, and in every other `.rs` file beside it, read from disk as
/// a server reads files nobody has open.
fn rename(text: &str, params: &serde_json::Value) -> serde_json::Value {
    let line = params
        .pointer("/position/line")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let at = params
        .pointer("/position/character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let new_name = params
        .get("newName")
        .and_then(|n| n.as_str())
        .unwrap_or_default();
    let uri = params
        .pointer("/textDocument/uri")
        .and_then(|u| u.as_str())
        .unwrap_or_default();
    let Some((_, _, word)) = word_at(text, line, at) else {
        return serde_json::Value::Null;
    };
    let mut changes = serde_json::Map::new();
    changes.insert(uri.to_string(), occurrences(text, &word, new_name).into());
    let here = uri
        .parse::<lsp_types::Uri>()
        .ok()
        .and_then(|u| crate::uri::uri_to_path(&u));
    let siblings = here
        .as_ref()
        .and_then(|p| std::fs::read_dir(p.parent()?).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs") && Some(p) != here.as_ref());
    for path in siblings {
        let (Ok(disk), Some(other)) = (
            std::fs::read_to_string(&path),
            crate::uri::path_to_uri(&path),
        ) else {
            continue;
        };
        let edits = occurrences(&disk, &word, new_name);
        if !edits.is_empty() {
            changes.insert(other.as_str().to_string(), edits.into());
        }
    }
    serde_json::json!({ "changes": changes })
}

/// What `textDocument/signatureHelp` answers: one signature, its parameter
/// the number of commas since the last open paren before the position. Null
/// outside any parens. The position is read as chars, which is exact for the
/// ASCII the tests type. The second parameter's label is an offset pair, the
/// other shape the protocol allows.
fn signature(text: &str, params: &serde_json::Value) -> serde_json::Value {
    let line = params
        .pointer("/position/line")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let at = params
        .pointer("/position/character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let line = text.split('\n').nth(line as usize).unwrap_or("");
    let before: String = line.chars().take(at as usize).collect();
    let Some(open) = before.rfind('(') else {
        return serde_json::Value::Null;
    };
    let active = before[open..].matches(',').count();
    serde_json::json!({
        "signatures": [{
            "label": "fn f(a: i32, b: &str)",
            "parameters": [{ "label": "a: i32" }, { "label": [13, 20] }],
        }],
        "activeSignature": 0,
        "activeParameter": active,
    })
}

/// Where the word ending at `at` on `line` starts, in UTF-8 bytes or UTF-16
/// units: the server's own count, which is the thing under test.
fn word_start(text: &str, line: usize, at: usize, utf8: bool) -> usize {
    let line = text.split('\n').nth(line).unwrap_or("");
    let width = |c: char| if utf8 { c.len_utf8() } else { c.len_utf16() };
    let mut seen = 0;
    let before: Vec<char> = line
        .chars()
        .take_while(|c| {
            seen += width(*c);
            seen <= at
        })
        .collect();
    let word: usize = before
        .iter()
        .rev()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .map(|c| width(*c))
        .sum();
    before.iter().map(|c| width(*c)).sum::<usize>() - word
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

#[cfg(windows)]
fn close_stdin() {
    use std::os::windows::io::AsRawHandle;
    // SAFETY: the handle is this process's stdin, nothing else reads it after
    // this, and it is closed once.
    unsafe { windows_sys::Win32::Foundation::CloseHandle(stdin().as_raw_handle()) };
}

#[cfg(unix)]
fn close_stdin() {
    // SAFETY: fd 0 is this process's stdin and nothing reads it after this.
    unsafe { libc::close(0) };
}

/// Read frames from stdin and answer them until told to exit.
pub fn run() {
    let flags = Flags::parse();

    if flags.exit_now {
        return;
    }

    if flags.close_stdin {
        // Break stdin while stdout stays open, so the client's writes fail and
        // its reader never sees end of stream. Closed in-process rather than by
        // exiting behind a grandchild: on Windows a grandchild inherits this
        // process's stdin handle and keeps the pipe alive.
        close_stdin();
        std::thread::park();
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
    let mut refusals = 0usize;
    // A completion answer waiting for the next `didChange`.
    let mut held: Option<lsp_server::RequestId> = None;
    // The document as last sent, which every sync kind here sends whole.
    let mut text = String::new();

    while let Ok(Some(message)) = Message::read(&mut input) {
        seen += 1;
        if flags.die_after == Some(seen) {
            return;
        }

        match message {
            Message::Request(Request { id, method, params }) => {
                let error = if method == "textDocument/prepareRename" && flags.refuse_rename {
                    Some((-32803, "this symbol cannot be renamed"))
                } else if method == "initialize" && flags.refuse_initialize {
                    Some((-32603, "this workspace is refused"))
                } else if method != "textDocument/hover" {
                    None
                } else if refusals < flags.content_modified {
                    refusals += 1;
                    Some((-32801, "content modified"))
                } else if flags.hover_error {
                    Some((-32603, "internal error"))
                } else {
                    None
                };
                if let Some((code, message)) = error {
                    let _ = Message::Response(Response {
                        id,
                        response_result: Err(lsp_server::ResponseError {
                            code,
                            message: message.into(),
                            data: None,
                        }),
                    })
                    .write(&mut out);
                    let _ = out.flush();
                    continue;
                }
                // The command asks the client to apply an edit, as a real
                // server's refactoring does, before answering.
                if method == "workspace/executeCommand" {
                    let uri = params
                        .pointer("/arguments/0")
                        .and_then(|u| u.as_str())
                        .unwrap_or_default();
                    let _ = Message::Request(Request {
                        id: APPLY_ID.into(),
                        method: "workspace/applyEdit".into(),
                        params: serde_json::json!({ "edit": fix_edit(uri, "ran") }),
                    })
                    .write(&mut out);
                }
                if method == "textDocument/completion" && flags.complete_late {
                    held = Some(id);
                    continue;
                }
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
                    "textDocument/completion" => completion(&flags, &text, &params),
                    "textDocument/signatureHelp" => signature(&text, &params),
                    "textDocument/codeAction" => code_actions(&params),
                    "textDocument/prepareRename" => prepare_rename(&text, &params),
                    "textDocument/rename" => rename(&text, &params),
                    "textDocument/hover" if flags.hover_empty => serde_json::Value::Null,
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
                if let Some(sent) = params
                    .pointer("/textDocument/text")
                    .or_else(|| params.pointer("/contentChanges/0/text"))
                    .and_then(|t| t.as_str())
                {
                    text = sent.to_string();
                }
                if method == "textDocument/didChange"
                    && let Some(id) = held.take()
                {
                    let _ = Message::Response(Response {
                        id,
                        response_result: Ok(completion(&flags, &text, &serde_json::Value::Null)),
                    })
                    .write(&mut out);
                    let _ = out.flush();
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
                    // Not LSP. Lets a test end a progress token at a moment it
                    // chooses rather than racing the server.
                    "fake/endProgress" if flags.progress => {
                        progress(&mut out, "indexing", serde_json::json!({ "kind": "end" }));
                    }
                    "textDocument/didOpen" if flags.push_malformed => {
                        let _ = Message::Notification(Notification {
                            method: "textDocument/publishDiagnostics".into(),
                            params: serde_json::json!({
                                "uri": open_uri,
                                "diagnostics": [{ "range": "line five", "message": "x" }],
                            }),
                        })
                        .write(&mut out);
                        let _ = out.flush();
                    }
                    // The text it was sent, back as a diagnostic message, so a
                    // test can compare the payload rather than count frames.
                    "textDocument/didOpen" | "textDocument/didChange" if flags.echo => {
                        let text = params
                            .pointer("/textDocument/text")
                            .or_else(|| params.pointer("/contentChanges/0/text"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("<no text>");
                        publish(&mut out, &open_uri, version, &[(0, 1, text)]);
                    }
                    "textDocument/didOpen" if flags.push_respelled => {
                        let respelled = open_uri.to_lowercase();
                        publish(&mut out, &respelled, version, &[(5, 1, "fake: respelled")]);
                    }
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
                if method == "initialized" && flags.progress {
                    if flags.progress_create {
                        // **And then wait.** The burst goes out when the client
                        // answers, not regardless of whether it does: sending
                        // it anyway is what let a client that answered nothing
                        // pass the test for this. See the `Response` arm.
                        let _ = Message::Request(Request {
                            id: CREATE_ID.into(),
                            method: "window/workDoneProgress/create".into(),
                            params: serde_json::json!({ "token": "indexing" }),
                        })
                        .write(&mut out);
                        let _ = out.flush();
                    } else {
                        progress_burst(&mut out);
                    }
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
            Message::Response(response) => {
                // The reply to `window/workDoneProgress/create`, and the burst
                // waits for it. A fake that sends progress regardless cannot
                // tell a client that answers from one that does not, and the
                // client did not answer at all. **An accepting reply**: a
                // client is entitled to refuse the token, and one that refuses
                // should not then be sent progress for it, so an error here is
                // as good as silence. Gaps 78 and 131.
                if flags.progress_create
                    && response.id == CREATE_ID.into()
                    && response.response_result.is_ok()
                {
                    progress_burst(&mut out);
                }
            }
        }
    }
}
