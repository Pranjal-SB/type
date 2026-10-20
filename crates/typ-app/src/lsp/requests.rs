//! The questions TYPE asks a server: what is in flight, which answer belongs
//! to which question, and the ordinary reasons there is no answer.
//!
//! Split from `mod.rs` at M4.6 because asking is its own responsibility beside
//! keeping servers running and documents in step, and the one completion and
//! signature help grow.

use std::path::{Path, PathBuf};

use ropey::Rope;
use typ_lsp::{RequestId, ServerId};

use super::{Lsp, config};

/// A request waiting for its answer.
pub(crate) struct Pending {
    pub id: RequestId,
    pub server: ServerId,
    pub kind: Ask,
    /// Where the cursor was when the question was asked.
    ///
    /// The generation lesson from M2.7's parses, applied to requests: an answer
    /// that arrives after the cursor moved describes somewhere the user is no
    /// longer asking about, and acting on it is a jump nobody requested.
    pub asked_at: (PathBuf, typ_buffer::Position),
    /// How many times this question has already been asked and cancelled.
    pub retries: u8,
}

/// Ask again this many times before giving up on a cancelled request.
///
/// The retry is paced by the round trip (the next one is not sent until the
/// last is answered) so no timer is involved and a spinning client is not
/// possible. The bound is against a server that cancels every time.
pub(crate) const MAX_RETRIES: u8 = 3;

/// `ContentModified`. Not a refusal: the server cancelled its own work and is
/// telling the client the question is still open.
pub(crate) const CONTENT_MODIFIED: i32 = -32801;

/// What was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    Definition,
    Hover,
}

impl Lsp {
    /// Ask the server for `path` a question about a position.
    ///
    /// Returns nothing when there is no server, when it has not finished the
    /// handshake, or when it says it cannot answer: all three are ordinary, and
    /// all three have to leave the editor exactly as it was.
    pub(crate) fn ask(
        &mut self,
        kind: Ask,
        path: &Path,
        char_index: usize,
        rope: &Rope,
        retries: u8,
    ) -> Result<(), NoAnswer> {
        let Some(doc) = self.docs.get(path) else {
            return Err(self.why_not_open(path));
        };
        let (id, uri) = (doc.server, doc.uri.clone());
        let encoding = self.encoding(id);
        let capability = match kind {
            Ask::Definition => "definitionProvider",
            Ask::Hover => "hoverProvider",
        };
        let method = match kind {
            Ask::Definition => "textDocument/definition",
            Ask::Hover => "textDocument/hover",
        };

        let Some(client) = self.client(id) else {
            return Err(NoAnswer::NoServer);
        };
        if !client.is_initialized() {
            return Err(NoAnswer::NotReady);
        }
        if !client.supports(capability) {
            return Err(NoAnswer::Unsupported);
        }

        let position = typ_lsp::to_lsp(encoding, rope.slice(..), char_index);
        let request = client.request(
            method,
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            }),
        );
        self.tally(method, true);
        self.pending.push(Pending {
            id: request,
            server: id,
            kind,
            asked_at: (path.to_path_buf(), typ_buffer::Position::default()),
            retries,
        });
        Ok(())
    }

    /// Why a path has no document on any server.
    ///
    /// A document is only recorded once the handshake completes, so "no
    /// document" covers a server still starting and one that refused the
    /// workspace as well as there being none. Gap 93.
    fn why_not_open(&self, path: &Path) -> NoAnswer {
        let started = self
            .configs
            .iter()
            .position(|c| c.handles(path))
            .and_then(|index| {
                let root = config::root_for(path, &self.configs[index].roots, &self.root);
                self.servers
                    .iter()
                    .find(|s| s.config == index && s.root == root)
            });
        match started.and_then(|s| s.client.as_deref()) {
            Some(client) if client.refusal().is_some() => NoAnswer::Refused,
            Some(client) if !client.is_initialized() => NoAnswer::NotReady,
            _ => NoAnswer::NoServer,
        }
    }

    /// Record where the cursor was for the request just sent.
    pub(crate) fn stamp_last(&mut self, at: typ_buffer::Position) {
        if let Some(last) = self.pending.last_mut() {
            last.asked_at.1 = at;
        }
    }

    /// Abandon any question of this kind that is still unanswered.
    ///
    /// Hover fires on demand today and on cursor movement later, so most
    /// questions are superseded before they are answered. A server still
    /// grinding on one nobody wants is burning a core for nothing.
    pub(crate) fn cancel(&mut self, kind: Ask) {
        let stale: Vec<(ServerId, RequestId)> = self
            .pending
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| (p.server, p.id.clone()))
            .collect();
        self.pending.retain(|p| p.kind != kind);
        for (server, id) in stale {
            if let Some(client) = self.client(server) {
                client.cancel(&id);
            }
            self.tally("$/cancelRequest", true);
        }
    }

    /// Take the question an answer belongs to, if anyone is still waiting.
    ///
    /// **A `RequestId` is only unique within one conversation.** Every `Client`
    /// numbers from zero, so rust-analyzer and taplo in one repository both
    /// issue `RequestId(2)`, and matching on the id alone handed one server's
    /// answer to the other server's question, running a hover payload through
    /// the goto-definition handler. `cancel` has always carried `p.server`;
    /// this had the field and did not read it. Gap 76.
    pub(crate) fn take_pending(&mut self, server: ServerId, id: &RequestId) -> Option<Pending> {
        let at = self
            .pending
            .iter()
            .position(|p| p.server == server && &p.id == id)?;
        Some(self.pending.remove(at))
    }
}

/// Why a question could not be asked.
///
/// Data rather than an error: every one of these is an ordinary state that the
/// status bar says a sentence about and nothing else happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoAnswer {
    /// Nothing is configured for this file, or it would not start.
    NoServer,
    /// It is up and has not finished the handshake.
    NotReady,
    /// It answered the handshake and does not offer this.
    Unsupported,
    /// It answered `initialize` with an error. Waiting will not help.
    Refused,
    /// It answered, and the answer was `null` or an empty list.
    ///
    /// A fourth case rather than silence. The first three happen before a
    /// request is sent and this one after, but from the keyboard they are the
    /// same event: a key was pressed and nothing appeared.
    Empty(Ask),
}

impl NoAnswer {
    /// What the status bar says.
    pub(crate) fn message(self) -> &'static str {
        match self {
            NoAnswer::NoServer => "No language server for this file.",
            NoAnswer::NotReady => "The language server is still starting.",
            NoAnswer::Unsupported => "This language server does not offer that.",
            NoAnswer::Refused => "The language server refused this workspace.",
            NoAnswer::Empty(Ask::Definition) => "No definition found.",
            NoAnswer::Empty(Ask::Hover) => "No hover information here.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Pending` is only ever built by `ask`, which needs a live server, so
    /// the correlation itself is reached by hand.
    fn pending(server: u32, id: i32, kind: Ask) -> Pending {
        Pending {
            id: RequestId::from(id),
            server: ServerId(server),
            kind,
            asked_at: (PathBuf::from("a.rs"), typ_buffer::Position::default()),
            retries: 0,
        }
    }

    #[test]
    fn an_answer_is_correlated_by_server_as_well_as_id() {
        // Every `Client` numbers its requests from zero, so two servers in one
        // repository (rust-analyzer for `.rs`, taplo for `.toml`) both issue
        // `RequestId(2)`. Matching on the id alone handed taplo's hover payload
        // to rust-analyzer's goto-definition handler. Gap 76.
        let mut lsp = Lsp::new(Path::new("."));
        lsp.pending.push(pending(0, 2, Ask::Definition));
        lsp.pending.push(pending(1, 2, Ask::Hover));

        let first = lsp
            .take_pending(ServerId(1), &RequestId::from(2))
            .expect("server 1 asked this");
        assert_eq!(first.kind, Ask::Hover, "took the other server's question");

        let second = lsp
            .take_pending(ServerId(0), &RequestId::from(2))
            .expect("server 0 is still waiting");
        assert_eq!(second.kind, Ask::Definition);
    }

    #[test]
    fn an_answer_from_a_server_nobody_asked_is_ignored() {
        let mut lsp = Lsp::new(Path::new("."));
        lsp.pending.push(pending(0, 2, Ask::Definition));
        assert!(lsp.take_pending(ServerId(9), &RequestId::from(2)).is_none());
        assert_eq!(lsp.pending.len(), 1, "the real question was consumed");
    }
}
