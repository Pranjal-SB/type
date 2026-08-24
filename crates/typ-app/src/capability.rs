//! What the terminal can actually show.
//!
//! Themes are written in truecolor. A terminal that cannot render it needs the
//! palette brought down to something it can, and that decision has to be made
//! once at startup rather than per cell.

/// Re-exported so callers detecting a depth and callers degrading a palette
/// name the same type. The enum lives in `typ-core` beside the degradation that
/// consumes it, because `typ-core` cannot depend on this crate.
pub use typ_core::Depth;

/// Decide from the three variables that carry the answer.
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
///
/// **`wt_session` is Windows Terminal, and it is an unconditional claim.**
/// Windows Terminal sets `WT_SESSION` to a session GUID and has historically
/// not set `COLORTERM`, so a stock install fell through to the 256-colour path
/// and quantised every theme for nothing — on the platform this editor is most
/// developed on. It renders 24-bit colour and has since it shipped, so the
/// presence of the variable is enough. oh-my-pi reads it the same way.
/// Set-but-empty is not a claim, for the same reason it is not one for
/// `COLORTERM`: scripts export variables unconditionally.
pub fn depth_from(colorterm: Option<&str>, term: Option<&str>, wt_session: Option<&str>) -> Depth {
    let claims_truecolor = colorterm.is_some_and(|value| {
        value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
    });
    let windows_terminal = wt_session.is_some_and(|value| !value.is_empty());
    // terminfo's convention for a direct-colour entry: `xterm-direct`,
    // `konsole-direct`, `vte-direct`. Unlike a `256color` suffix this is an
    // unambiguous 24-bit claim, and it is the one thing `TERM` says about colour
    // that can be relied on.
    let direct_colour_entry = term.is_some_and(|value| value.contains("-direct"));

    if claims_truecolor || direct_colour_entry || windows_terminal {
        return Depth::TrueColor;
    }

    // Everything else, including "nothing set at all". 256 is the floor rather
    // than a 16-colour path, because there is no sane mapping onto the sixteen —
    // see `typ_core::colour::downgrade`. A terminal that cannot manage 256
    // cannot manage the rest of this editor either, so there is nothing below
    // this to fall back to.
    Depth::Ansi256
}

/// The only thing here that reads the environment.
///
/// No test covers it, and none should: there is no logic in it beyond handing
/// three variables to `depth_from`.
pub fn detect() -> Depth {
    let colorterm = std::env::var("COLORTERM").ok();
    let term = std::env::var("TERM").ok();
    let wt_session = std::env::var("WT_SESSION").ok();
    depth_from(colorterm.as_deref(), term.as_deref(), wt_session.as_deref())
}
