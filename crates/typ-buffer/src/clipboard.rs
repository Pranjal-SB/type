//! The clipboard: an internal register, the system clipboard behind it, and
//! OSC 52 in front of both.
//!
//! # Why not a clipboard crate
//!
//! X11 has no clipboard daemon. A selection is owned by a *live process*, and
//! when that process exits the content is gone unless a clipboard manager
//! happened to claim it. `xclip` and `wl-copy` fork a background process
//! specifically to hold that ownership; a library that sets the selection from
//! inside this process requires this process to stay alive to serve it. For an
//! editor the failure is: copy, quit, paste elsewhere, get nothing.
//!
//! Helix reaches for external commands for exactly this reason, citing Neovim's
//! `provider/clipboard.vim`; oh-my-pi does the same and keeps a PowerShell path
//! on Windows. Three implementations agreeing is enough evidence.
//!
//! # The layers
//!
//! 1. **The register.** Always present, always the source of truth for paste
//!    inside TYPE. Nothing can make this fail.
//! 2. **OSC 52 on write, emitted first.** An escape sequence carrying base64
//!    that the *local* terminal intercepts, so a copy over SSH lands in the
//!    laptop's clipboard rather than the server's.
//! 3. **A command provider**, chosen by environment variable and binary
//!    presence rather than by trying and catching.
//!
//! # Reading
//!
//! There is no OSC 52 read. The reply has to be parsed off the input stream,
//! and terminals disable clipboard *reads* by default for good reason — a
//! remote host that can read your clipboard reads whatever you last copied.
//! Reads go to the provider, then fall back to the register.
//!
//! # Failure
//!
//! Nothing here surfaces an error. A headless box with no clipboard is a normal
//! condition, not something to interrupt someone about.

use std::io::Write;
#[cfg(not(windows))]
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// The internal register. Process-wide because the clipboard is.
fn cell() -> &'static Mutex<String> {
    static CELL: OnceLock<Mutex<String>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(String::new()))
}

/// Whether to talk to the system clipboard at all.
///
/// Off by default and switched on by the binary at startup, so a test suite
/// never spawns `wl-copy` or clobbers whatever the developer had copied. A
/// library that reaches for the machine's clipboard the moment it is linked is
/// a library that cannot be tested politely.
fn system_enabled() -> &'static Mutex<bool> {
    static ENABLED: OnceLock<Mutex<bool>> = OnceLock::new();
    ENABLED.get_or_init(|| Mutex::new(false))
}

/// Let the clipboard reach the system. Called once, by the binary.
pub fn enable_system() {
    *system_enabled().lock().unwrap() = true;
}

fn system_is_enabled() -> bool {
    *system_enabled().lock().unwrap()
}

/// What the register holds.
pub fn register() -> String {
    cell().lock().unwrap().clone()
}

/// Set the register alone, touching nothing outside this process.
pub fn set_register(text: &str) {
    *cell().lock().unwrap() = text.to_string();
}

/// Copy: register, then OSC 52, then the system provider.
///
/// The register is set first and unconditionally, so a paste inside TYPE works
/// even when every outward path fails.
pub fn set(text: &str) {
    set_register(text);
    if !system_is_enabled() {
        return;
    }
    emit_osc52(text);
    Provider::detect().set(text);
}

/// Paste: the system provider, falling back to the register.
///
/// The provider wins when it answers, so text copied in another application is
/// available here. An empty answer counts as no answer — every provider prints
/// nothing when the clipboard holds nothing, which is indistinguishable from
/// failing, and preferring the register in that case is the more useful guess.
pub fn get() -> String {
    if system_is_enabled() {
        let external = Provider::detect().get();
        if !external.is_empty() {
            return external;
        }
    }
    register()
}

/// Write the selection to the terminal as OSC 52.
///
/// `\x1b]52;c;<base64>\x07` — `c` is the clipboard selection. Terminals that do
/// not support it ignore the sequence, and the write is best-effort: stdout may
/// legitimately not be a terminal.
fn emit_osc52(text: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

/// Base64, by hand.
///
/// Twenty lines against a dependency that would be pulled in for one call site.
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// How this machine talks to its clipboard.
///
/// Ordered by preference and detected from the environment, following Helix.
/// Each platform carries only the variants it can actually construct — Windows
/// goes straight to the native API and never shells out, so listing `XClip`
/// there would be a variant that exists to be dead.
///
/// `Primary` — the X11 middle-click selection — is deliberately absent: it is a
/// second clipboard rather than a second provider, and it lands with the mouse
/// work at M4.
enum Provider {
    #[cfg(windows)]
    Windows,
    #[cfg(target_os = "macos")]
    Pasteboard,
    #[cfg(not(windows))]
    Wayland,
    #[cfg(not(windows))]
    XClip,
    #[cfg(not(windows))]
    XSel,
    #[cfg(not(windows))]
    Tmux,
    #[cfg(not(windows))]
    Termux,
    #[cfg(not(windows))]
    None,
}

/// Is this binary on the PATH?
///
/// `command -v` rather than a `which` crate: the shell already answers this,
/// and detection runs once per process.
#[cfg(not(windows))]
fn has(binary: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {binary}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

impl Provider {
    /// Detected once. The environment does not change under a running editor,
    /// and probing for four binaries on every copy would be absurd.
    fn detect() -> &'static Self {
        static PROVIDER: OnceLock<Provider> = OnceLock::new();
        PROVIDER.get_or_init(Self::detect_uncached)
    }

    #[cfg(windows)]
    fn detect_uncached() -> Self {
        // The native API, always. There is no Windows equivalent of the
        // ownership problem that makes shelling out the right answer on X11,
        // and `clip.exe` writes in the console codepage — which mangles
        // anything outside it, in an editor built on grapheme correctness.
        Self::Windows
    }

    #[cfg(not(windows))]
    fn detect_uncached() -> Self {
        // Inside tmux, tmux owns the clipboard regardless of what is underneath.
        if env_set("TMUX") && has("tmux") {
            return Self::Tmux;
        }
        if has("termux-clipboard-set") {
            return Self::Termux;
        }

        #[cfg(target_os = "macos")]
        if has("pbcopy") {
            return Self::Pasteboard;
        }

        if env_set("WAYLAND_DISPLAY") && has("wl-copy") {
            return Self::Wayland;
        }
        if env_set("DISPLAY") && has("xclip") {
            return Self::XClip;
        }
        if env_set("DISPLAY") && has("xsel") {
            return Self::XSel;
        }
        // OSC 52 already went out in `set`, so no provider is a working
        // configuration rather than a broken one.
        Self::None
    }

    /// The command that writes, and the one that reads.
    #[cfg(not(windows))]
    fn commands(&self) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
        match self {
            Self::Pasteboard => Some((vec!["pbcopy"], vec!["pbpaste"])),
            Self::Wayland => Some((
                vec!["wl-copy", "--foreground", "--type", "text/plain"],
                vec!["wl-paste", "--no-newline"],
            )),
            Self::XClip => Some((
                vec!["xclip", "-i", "-selection", "clipboard"],
                vec!["xclip", "-o", "-selection", "clipboard"],
            )),
            Self::XSel => Some((vec!["xsel", "-i", "-b"], vec!["xsel", "-o", "-b"])),
            Self::Tmux => Some((
                vec!["tmux", "load-buffer", "-w", "-"],
                vec!["tmux", "save-buffer", "-"],
            )),
            Self::Termux => Some((vec!["termux-clipboard-set"], vec!["termux-clipboard-get"])),
            Self::None => None,
        }
    }

    #[cfg(windows)]
    fn set(&self, text: &str) {
        let _ = clipboard_win::set_clipboard(clipboard_win::formats::Unicode, text);
    }

    #[cfg(not(windows))]
    fn set(&self, text: &str) {
        let Some((write, _)) = self.commands() else {
            return;
        };
        let Ok(mut child) = Command::new(write[0])
            .args(&write[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        // Dropping stdin closes it, which is what tells the provider the write
        // is finished; without it `wl-copy --foreground` waits forever.
        drop(child.stdin.take());
        let _ = child.wait();
    }

    #[cfg(windows)]
    fn get(&self) -> String {
        clipboard_win::get_clipboard(clipboard_win::formats::Unicode).unwrap_or_default()
    }

    #[cfg(not(windows))]
    fn get(&self) -> String {
        let Some((_, read)) = self.commands() else {
            return String::new();
        };
        Command::new(read[0])
            .args(&read[1..])
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_rfc_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_non_ascii() {
        // The padding maths is where a hand-rolled encoder goes wrong, and
        // multibyte input is what exercises it.
        assert_eq!(base64("é".as_bytes()), "w6k=");
        assert_eq!(base64("日本".as_bytes()), "5pel5pys");
    }
}
