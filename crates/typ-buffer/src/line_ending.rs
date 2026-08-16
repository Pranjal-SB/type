//! Which line terminator a file uses.
//!
//! Detected on load, recorded here, and written back by `save`. The rope holds
//! LF only, so nothing between those two points has to know about `\r`.

/// The line terminator a buffer was loaded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    /// `\n`. The default for a new or newline-free file on every platform,
    /// including Windows — a file TYPE creates has no existing convention to
    /// honour, and LF is what the tools around it emit.
    #[default]
    Lf,
    /// `\r\n`.
    Crlf,
}

impl LineEnding {
    /// What a status bar shows. The names every editor uses.
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
        }
    }

    /// The characters themselves, as `save` writes them.
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
        }
    }

    /// Detect from a file's contents.
    ///
    /// The **first** terminator decides. A mixed file is not a third kind of
    /// file: whatever line one did is what the file is, and it is what gets
    /// written back. Taking a majority instead would mean a save that silently
    /// rewrites every line break in somebody's file because the count went the
    /// other way.
    ///
    /// A lone `\r` is not a line ending. Classic Mac endings died with Mac OS 9
    /// and a stray carriage return inside a line is far likelier, so treating
    /// one as a terminator would misread an ordinary file badly.
    pub fn detect(text: &str) -> Self {
        match text.find('\n') {
            Some(0) => LineEnding::Lf,
            Some(index) if text.as_bytes()[index - 1] == b'\r' => LineEnding::Crlf,
            Some(_) => LineEnding::Lf,
            None => LineEnding::Lf,
        }
    }
}
