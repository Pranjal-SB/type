//! LSP positions and char offsets, in all three encodings.
//!
//! **Char offsets in, char offsets out.** A char is ropey's native unit and the
//! natural pivot for every encoding LSP defines; `col` is a grapheme index and
//! `typ-buffer` converts, because that is where grapheme logic already lives.
//!
//! UTF-16 is the only encoding a server must support and most implement nothing
//! else, so it is the path that has to be fast as well as right. It is not
//! counted by hand: ropey keeps a surrogate count in its tree and answers
//! `char_to_utf16_cu` in O(log N).

use ropey::RopeSlice;

/// Which unit a server counts `Position::character` in.
///
/// TYPE's own enum rather than `lsp_types::PositionEncodingKind`, which is a
/// newtype over a string: a match here is exhaustive, and a fourth encoding
/// would be a compiler error rather than a silent fallthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Bytes. Cheap here — ropey indexes bytes natively.
    Utf8,
    /// UTF-16 code units. The default, and mandatory for servers.
    Utf16,
    /// Unicode code points, which are exactly ropey's chars. Free.
    Utf32,
}

impl Encoding {
    /// The wire name, for `general.positionEncodings`.
    pub fn as_str(self) -> &'static str {
        match self {
            Encoding::Utf8 => "utf-8",
            Encoding::Utf16 => "utf-16",
            Encoding::Utf32 => "utf-32",
        }
    }

    /// What a server's answer means, or `None` if it named something else.
    pub fn from_wire(name: &str) -> Option<Encoding> {
        match name {
            "utf-8" => Some(Encoding::Utf8),
            "utf-16" => Some(Encoding::Utf16),
            "utf-32" => Some(Encoding::Utf32),
            _ => None,
        }
    }
}

/// Chars in `line` before its line break, if it has one.
///
/// A cursor cannot sit between the `\r` and the `\n` of a CRLF pair, so the end
/// of a line is the start of its break rather than any offset inside it. Every
/// break ropey recognises is handled, not just `\n`, because ropey's line
/// indexing already counts them all and disagreeing with it here would put the
/// two out of step on exactly the files that are hardest to debug.
fn content_len(line: RopeSlice) -> usize {
    let n = line.len_chars();
    if n == 0 {
        return 0;
    }
    match line.char(n - 1) {
        '\n' if n >= 2 && line.char(n - 2) == '\r' => n - 2,
        '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => n - 1,
        _ => n,
    }
}

/// Where `char_idx` is, in the units this server counts.
pub fn to_lsp(encoding: Encoding, rope: RopeSlice, char_idx: usize) -> lsp_types::Position {
    let char_idx = char_idx.min(rope.len_chars());
    let line = rope.char_to_line(char_idx);
    let start = rope.line_to_char(line);

    let character = match encoding {
        Encoding::Utf32 => char_idx - start,
        Encoding::Utf8 => rope.char_to_byte(char_idx) - rope.char_to_byte(start),
        Encoding::Utf16 => rope.char_to_utf16_cu(char_idx) - rope.char_to_utf16_cu(start),
    };

    lsp_types::Position {
        line: line as u32,
        character: character as u32,
    }
}

/// The char offset `pos` names.
///
/// **Clamps rather than panicking, at every step.** A position is remote input:
/// it may name a line past the end of the file the server last saw, a column
/// past the end of that line, or an offset inside a surrogate pair. Ropey's
/// `utf16_cu_to_char` and `byte_to_char` panic out of bounds, so the clamping
/// happens before the call and not after.
pub fn from_lsp(encoding: Encoding, rope: RopeSlice, pos: lsp_types::Position) -> usize {
    let last_line = rope.len_lines().saturating_sub(1);
    let line = pos.line as usize;
    if line > last_line {
        return rope.len_chars();
    }

    let start = rope.line_to_char(line);
    let end = start + content_len(rope.line(line));
    let character = pos.character as usize;

    let idx = match encoding {
        Encoding::Utf32 => start + character,
        Encoding::Utf8 => {
            let byte = (rope.char_to_byte(start) + character).min(rope.len_bytes());
            rope.byte_to_char(byte)
        }
        Encoding::Utf16 => {
            let cu = (rope.char_to_utf16_cu(start) + character).min(rope.len_utf16_cu());
            rope.utf16_cu_to_char(cu)
        }
    };

    idx.min(end)
}
