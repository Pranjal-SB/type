//! What the terminal can actually show.
//!
//! Themes are written in truecolor. A terminal that cannot render it needs the
//! palette brought down to something it can, and that decision has to be made
//! once at startup rather than per cell.

/// Re-exported so callers detecting a depth and callers degrading a palette
/// name the same type. The enum lives in `typ-core` beside the degradation that
/// consumes it, because `typ-core` cannot depend on this crate.
pub use crate::backend::Underlines;
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

/// Whether this terminal draws a curly underline, from the variables that hint
/// at it.
///
/// Pure for the same reason `depth_from` is: environment is global, cargo runs
/// tests in threads of one process, and a test that sets `TERM` is a test that
/// changes what its siblings see.
///
/// **This is an allowlist and it will be wrong for somebody.** The right signal
/// is terminfo's `Smulx`, TYPE has no terminfo reader, and adding one for a
/// single capability whose failure mode is a straight underline is not a trade
/// worth making. The list comes from checking each terminal's own source or
/// release notes rather than from one aggregator: kitty invented the sequence,
/// and VTE, WezTerm, foot, Ghostty, iTerm2, Konsole, Alacritty, contour,
/// mintty, Windows Terminal and xterm.js have each since implemented it. xterm,
/// PuTTY, rxvt-unicode, st and GNU screen have not.
///
/// **A multiplexer answers no**, which is the opposite of what `depth_from`
/// does with `COLORTERM`, and deliberately. tmux 2.9 and Zellij 0.39 both pass
/// `4:3` through, but only if the terminal underneath draws it — and inside a
/// multiplexer nothing in the environment names that terminal. A truecolor
/// claim inherited into tmux is usually still true; a *shape* claim is not
/// something tmux inherits at all, so there is nothing to believe. The cost of
/// answering no is a straight underline; the cost of answering yes wrongly is
/// a literal `4:3m` printed into the buffer on a terminal that does not parse
/// subparameters.
///
/// Known misses, listed rather than hidden: Konsole sets no variable naming
/// itself that is safe to key on, and a terminal reached through `ssh` carries
/// only `TERM`. Both get a plain underline, which is the point of the fallback.
pub fn underlines_from(
    term: Option<&str>,
    term_program: Option<&str>,
    wt_session: Option<&str>,
    vte_version: Option<&str>,
) -> Underlines {
    let term = term.unwrap_or_default();

    // Inside tmux or screen, `TERM` describes the multiplexer and every other
    // variable describes whatever started it, which may be two terminals ago.
    if term.starts_with("tmux") || term.starts_with("screen") {
        return Underlines::Plain;
    }

    // Windows Terminal, which draws it and has since 2024. It is also the one
    // terminal that made the *colour* sequence a compatibility question —
    // see `backend::underline_colour`.
    if wt_session.is_some_and(|value| !value.is_empty()) {
        return Underlines::Styled;
    }

    // GNOME Terminal and the rest of the VTE family set this to a build
    // number. Styled underlines landed in 0.52; anything setting the variable
    // at all is far past that by now, so its presence is the claim.
    if vte_version.is_some_and(|value| !value.is_empty()) {
        return Underlines::Styled;
    }

    let program_says_yes = term_program.is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "wezterm" | "iterm.app" | "ghostty" | "mintty" | "contour" | "rio" | "vscode"
        )
    });
    if program_says_yes {
        return Underlines::Styled;
    }

    // `TERM` last, because it is the least trustworthy of the four — but these
    // four values are set by the terminal itself and name it exactly.
    let term_says_yes = term.starts_with("xterm-kitty")
        || term.starts_with("foot")
        || term.starts_with("wezterm")
        || term.starts_with("alacritty")
        || term.starts_with("xterm-ghostty")
        || term.starts_with("contour");

    if term_says_yes {
        Underlines::Styled
    } else {
        Underlines::Plain
    }
}

/// The only thing here that reads the environment for underlines.
///
/// No test covers it, and none should — see `detect`.
pub fn detect_underlines() -> Underlines {
    let term = std::env::var("TERM").ok();
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let wt_session = std::env::var("WT_SESSION").ok();
    let vte_version = std::env::var("VTE_VERSION").ok();
    underlines_from(
        term.as_deref(),
        term_program.as_deref(),
        wt_session.as_deref(),
        vte_version.as_deref(),
    )
}
