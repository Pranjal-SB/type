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
            depth_from(Some(value), Some("xterm-256color")),
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
        depth_from(Some("truecolor"), Some("xterm-256color")),
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
            depth_from(None, Some(term)),
            Depth::TrueColor,
            "TERM={term} is a direct-colour entry"
        );
    }
}

#[test]
fn a_256_colour_term_without_colorterm_gets_256() {
    for term in ["xterm-256color", "screen-256color", "tmux-256color"] {
        assert_eq!(
            depth_from(None, Some(term)),
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
    assert_eq!(depth_from(None, Some("dumb")), Depth::Ansi256);
    assert_eq!(depth_from(None, Some("xterm")), Depth::Ansi256);
    assert_eq!(depth_from(None, Some("vt100")), Depth::Ansi256);
}

#[test]
fn nothing_set_at_all_is_the_safe_answer() {
    // A cron job, a CI runner, a pipe. Claiming 24-bit here and being wrong
    // produces a screen of escape sequences; claiming 256 does not.
    assert_eq!(depth_from(None, None), Depth::Ansi256);
}

#[test]
fn an_empty_colorterm_is_not_a_claim() {
    // Set-but-empty is common in scripts that export it unconditionally, and it
    // says nothing. Treating it as truthy is how a 16-colour terminal gets sent
    // 24-bit sequences.
    assert_eq!(depth_from(Some(""), Some("xterm-256color")), Depth::Ansi256);
    assert_eq!(depth_from(Some(""), None), Depth::Ansi256);
}

#[test]
fn an_unrecognised_colorterm_value_does_not_claim_truecolor() {
    // Some terminals set COLORTERM to their own name. That is not a capability
    // statement, and reading it as one over-claims.
    assert_eq!(
        depth_from(Some("rxvt-xpm"), Some("xterm-256color")),
        Depth::Ansi256
    );
    assert_eq!(depth_from(Some("gnome-terminal"), None), Depth::Ansi256);
}
