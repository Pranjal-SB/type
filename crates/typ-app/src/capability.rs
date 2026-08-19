//! What the terminal can actually show.
//!
//! Themes are written in truecolor. A terminal that cannot render it needs the
//! palette brought down to something it can, and that decision has to be made
//! once at startup rather than per cell.

/// How many colours the terminal can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// 24-bit. Themes render as written.
    TrueColor,
    /// The 6×6×6 cube plus the grey ramp. Every theme colour is quantised.
    Ansi256,
    /// The sixteen the user's terminal defines. TYPE cannot tune these at all —
    /// whatever "blue" means here is whatever they set it to.
    Ansi16,
}

/// Decide from the two variables that carry the answer.
///
/// Pure, and every input is an argument, because the alternative is a test that
/// reads process environment. Environment is global and cargo runs tests in
/// parallel threads inside one process, so a test setting `COLORTERM` is a test
/// that changes what its siblings see. `tests/logging.rs` already needs
/// `unsafe { set_var }` for exactly this reason. A pure function means the
/// problem does not exist rather than being guarded against.
///
/// `COLORTERM` is checked before `TERM` because `TERM` rarely encodes truecolor
/// accurately and terminfo's `RGB` capability is unevenly populated across
/// distributions. `truecolor` and `24bit` are the two de-facto values, and every
/// modern terminal — kitty, WezTerm, Ghostty, foot, iTerm2, Alacritty, Windows
/// Terminal — sets one of them.
///
/// **A multiplexer is not special-cased, deliberately.** tmux is the usual place
/// `COLORTERM=truecolor` is inherited from the outer environment while truecolor
/// does not survive to the terminal, which argues for capping `screen*`/`tmux*`
/// at 256. But tmux 3.2 and later, told `set -as terminal-features ",*:RGB"`,
/// passes it through correctly — and nothing in the environment distinguishes a
/// configured tmux from an unconfigured one. Capping punishes the configured
/// case, and "renders in 256-colour mode inside tmux despite COLORTERM=truecolor"
/// is a bug report other terminal programs have already collected.
///
/// So the claim is believed, and the escape hatch is a setting rather than a
/// better guess. `config.toml` gets `color_depth` when it lands.
pub fn depth_from(colorterm: Option<&str>, term: Option<&str>) -> Depth {
    let claims_truecolor = colorterm.is_some_and(|value| {
        value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
    });
    if claims_truecolor {
        return Depth::TrueColor;
    }

    if term.is_some_and(|value| value.contains("256color")) {
        return Depth::Ansi256;
    }

    Depth::Ansi16
}

/// The only thing here that reads the environment.
///
/// No test covers it, and none should: there is no logic in it beyond handing
/// two variables to `depth_from`.
pub fn detect() -> Depth {
    let colorterm = std::env::var("COLORTERM").ok();
    let term = std::env::var("TERM").ok();
    depth_from(colorterm.as_deref(), term.as_deref())
}
