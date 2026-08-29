//! What the terminal reports, and what TYPE concludes from it.
//!
//! Every case here is a pair of strings in and a `Depth` out. **No test in this
//! file reads or writes process environment**, and that is the point of
//! `depth_from` taking its inputs as arguments: cargo runs tests in parallel
//! threads inside one process, so a test that sets `COLORTERM` changes what its
//! siblings see while they are running.

use typ_app::capability::{Depth, depth_from};

#[test]
fn colorterm_is_believed_when_it_claims_truecolor() {
    // The two de-facto values, and the case a terminal might actually write.
    for value in ["truecolor", "24bit", "TrueColor", "24BIT"] {
        assert_eq!(
            depth_from(Some(value), Some("xterm-256color"), None),
            Depth::TrueColor,
            "COLORTERM={value} claims 24-bit"
        );
    }
}

#[test]
fn colorterm_outranks_term() {
    // TERM says 256, COLORTERM says 24-bit, and COLORTERM wins: TERM rarely
    // encodes truecolor accurately and terminfo's RGB capability is unevenly
    // populated, so the more specific claim is the one to believe.
    assert_eq!(
        depth_from(Some("truecolor"), Some("xterm-256color"), None),
        Depth::TrueColor
    );
}

#[test]
fn a_direct_colour_terminfo_entry_is_a_truecolor_claim() {
    // terminfo's `-direct` entries mean 24-bit, and unlike a `256color` suffix
    // that is unambiguous. It is the one thing TERM says about colour worth
    // acting on.
    for term in ["xterm-direct", "konsole-direct", "vte-direct"] {
        assert_eq!(
            depth_from(None, Some(term), None),
            Depth::TrueColor,
            "TERM={term} is a direct-colour entry"
        );
    }
}

#[test]
fn a_256_colour_term_without_colorterm_gets_256() {
    for term in ["xterm-256color", "screen-256color", "tmux-256color"] {
        assert_eq!(
            depth_from(None, Some(term), None),
            Depth::Ansi256,
            "TERM={term} advertises 256 colours"
        );
    }
}

#[test]
fn a_terminal_that_advertises_nothing_still_gets_256() {
    // 256 is the floor, not a fallback to something smaller. There is no
    // 16-colour depth to fall back *to*: nothing maps twenty-four designed
    // colours onto seven hues at two lightnesses without breaking somewhere,
    // and a terminal that cannot do 256 cannot do the rest of this editor.
    assert_eq!(depth_from(None, Some("dumb"), None), Depth::Ansi256);
    assert_eq!(depth_from(None, Some("xterm"), None), Depth::Ansi256);
    assert_eq!(depth_from(None, Some("vt100"), None), Depth::Ansi256);
}

#[test]
fn nothing_set_at_all_is_the_safe_answer() {
    // A cron job, a CI runner, a pipe. Claiming 24-bit here and being wrong
    // produces a screen of escape sequences; claiming 256 does not.
    assert_eq!(depth_from(None, None, None), Depth::Ansi256);
}

#[test]
fn an_empty_colorterm_is_not_a_claim() {
    // Set-but-empty is common in scripts that export it unconditionally, and it
    // says nothing. Treating it as truthy is how a 16-colour terminal gets sent
    // 24-bit sequences.
    assert_eq!(
        depth_from(Some(""), Some("xterm-256color"), None),
        Depth::Ansi256
    );
    assert_eq!(depth_from(Some(""), None, None), Depth::Ansi256);
}

#[test]
fn an_unrecognised_colorterm_value_does_not_claim_truecolor() {
    // Some terminals set COLORTERM to their own name. That is not a capability
    // statement, and reading it as one over-claims.
    assert_eq!(
        depth_from(Some("rxvt-xpm"), Some("xterm-256color"), None),
        Depth::Ansi256
    );
    assert_eq!(
        depth_from(Some("gnome-terminal"), None, None),
        Depth::Ansi256
    );
}

#[test]
fn windows_terminal_claims_truecolor_without_colorterm() {
    // Defect #43. Windows Terminal sets WT_SESSION to a session GUID and has
    // historically not set COLORTERM, so a stock install fell to the
    // 256-colour path and every theme was quantised for nothing — on the
    // platform this editor is most developed on.
    assert_eq!(
        depth_from(None, None, Some("abc-123")),
        Depth::TrueColor,
        "WT_SESSION is Windows Terminal, which is truecolor"
    );
}

#[test]
fn windows_terminal_outranks_a_term_that_says_less() {
    // TERM under Windows Terminal is whatever the shell decided, often
    // `xterm-256color` — and it is wrong. The more specific signal wins, the
    // same way COLORTERM outranks TERM.
    assert_eq!(
        depth_from(None, Some("xterm-256color"), Some("guid")),
        Depth::TrueColor
    );
    assert_eq!(
        depth_from(None, Some("xterm"), Some("guid")),
        Depth::TrueColor
    );
}

#[test]
fn an_empty_wt_session_is_not_a_claim() {
    // Set-but-empty says nothing, exactly as it does for COLORTERM. A script
    // that exports the variable unconditionally must not thereby promise
    // 24-bit colour.
    assert_eq!(depth_from(None, Some("xterm"), Some("")), Depth::Ansi256);
    assert_eq!(depth_from(None, None, Some("")), Depth::Ansi256);
}

#[test]
fn the_existing_signals_still_win_on_their_own() {
    // The third argument is additive. Every conclusion reachable before it
    // existed is still reachable without it.
    assert_eq!(depth_from(Some("truecolor"), None, None), Depth::TrueColor);
    assert_eq!(
        depth_from(None, Some("xterm-direct"), None),
        Depth::TrueColor
    );
    assert_eq!(
        depth_from(None, Some("xterm-256color"), None),
        Depth::Ansi256
    );
    assert_eq!(depth_from(None, None, None), Depth::Ansi256);
}

// --- styled underlines ---

use typ_app::capability::{Underlines, underlines_from};

fn underlines(term: &str) -> Underlines {
    underlines_from(Some(term), None, None, None)
}

#[test]
fn kitty_foot_wezterm_alacritty_and_ghostty_draw_a_curl() {
    for term in [
        "xterm-kitty",
        "foot",
        "foot-extra",
        "wezterm",
        "alacritty",
        "xterm-ghostty",
    ] {
        assert_eq!(underlines(term), Underlines::Styled, "{term}");
    }
}

#[test]
fn xterm_and_the_rest_get_a_straight_line() {
    for term in ["xterm", "xterm-256color", "rxvt-unicode", "st-256color", ""] {
        assert_eq!(underlines(term), Underlines::Plain, "{term}");
    }
}

#[test]
fn windows_terminal_draws_a_curl() {
    // The platform this editor is most developed on, and the one that made the
    // *colour* sequence a compatibility question.
    assert_eq!(
        underlines_from(None, None, Some("some-guid"), None),
        Underlines::Styled
    );
    // Set-but-empty is not a claim, for the same reason it is not one for
    // `COLORTERM`: scripts export variables unconditionally.
    assert_eq!(
        underlines_from(None, None, Some(""), None),
        Underlines::Plain
    );
}

#[test]
fn a_vte_build_number_is_a_claim() {
    assert_eq!(
        underlines_from(Some("xterm-256color"), None, None, Some("7402")),
        Underlines::Styled
    );
}

#[test]
fn a_named_terminal_program_is_believed() {
    for program in ["WezTerm", "iTerm.app", "ghostty", "mintty", "vscode"] {
        assert_eq!(
            underlines_from(Some("xterm-256color"), Some(program), None, None),
            Underlines::Styled,
            "{program}"
        );
    }
    assert_eq!(
        underlines_from(Some("xterm-256color"), Some("Apple_Terminal"), None, None),
        Underlines::Plain,
        "Terminal.app has never drawn one"
    );
}

#[test]
fn a_multiplexer_answers_no_whatever_else_is_set() {
    // Inside tmux, every other variable describes whichever terminal started
    // it, which may be two terminals ago. A shape claim is not something tmux
    // inherits, so there is nothing to believe.
    for term in ["tmux-256color", "screen-256color"] {
        assert_eq!(
            underlines_from(Some(term), Some("WezTerm"), Some("guid"), Some("7402")),
            Underlines::Plain,
            "{term}"
        );
    }
}
