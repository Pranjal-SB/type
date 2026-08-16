//! The status-bar prompt.
//!
//! One line, one purpose at a time. It exists because M1.2 proved the editor
//! needs somewhere to ask a question; search and replace are the second and
//! third questions it asks.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Search,
    /// The needle has been entered; this is collecting the replacement.
    Replace,
    /// A line number to jump to.
    GotoLine,
}

#[derive(Debug, Clone)]
pub struct Prompt {
    kind: PromptKind,
    input: String,
    /// Set while a replace is collecting its second answer.
    pending_needle: Option<String>,
    /// True when this prompt was opened by Ctrl+H, so answering the needle
    /// leads to a second question rather than to a jump.
    replace_flow: bool,
}

impl Prompt {
    pub fn new(kind: PromptKind) -> Self {
        Self {
            kind,
            input: String::new(),
            pending_needle: None,
            replace_flow: false,
        }
    }

    pub fn kind(&self) -> PromptKind {
        self.kind
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn label(&self) -> &'static str {
        match self.kind {
            PromptKind::Search => "Search:",
            PromptKind::Replace => "Replace with:",
            PromptKind::GotoLine => "Go to line:",
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.input.push(c);
    }

    /// Remove one grapheme, not one byte or char — the prompt accepts the same
    /// text the buffer does, including CJK and combining sequences.
    pub fn delete_backward(&mut self) {
        let mut graphemes: Vec<&str> = self.input.graphemes(true).collect();
        graphemes.pop();
        self.input = graphemes.concat();
    }

    /// Put an answer back after rejecting it.
    ///
    /// A prompt that clears itself on a typo makes the user retype the whole
    /// thing, which is the annoying version of validation.
    pub fn restore_input(&mut self, input: String) {
        self.input = input;
    }

    pub fn take_input(&mut self) -> String {
        std::mem::take(&mut self.input)
    }

    pub fn set_pending_needle(&mut self, needle: String) {
        self.pending_needle = Some(needle);
    }

    pub fn pending_needle(&self) -> Option<&str> {
        self.pending_needle.as_deref()
    }

    /// Mark this as the first half of a replace, so Enter collects the needle
    /// and asks for the replacement instead of jumping to the match.
    pub fn become_replace_after_needle(&mut self) {
        self.replace_flow = true;
    }

    pub fn is_replace_flow(&self) -> bool {
        self.replace_flow
    }

    /// Move to the second question, keeping the prompt open.
    ///
    /// A separate prompt type per question would double the state for no gain:
    /// the only thing that changes is the label and where the answer goes.
    pub fn become_replace(&mut self) {
        self.kind = PromptKind::Replace;
        self.replace_flow = false;
    }
}
