use std::any::Any;

use crossterm::event::MouseEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::{KeyChord, PanelEvent};

/// The shipped palette, as a named ramp rather than a colour per widget.
///
/// Architecture §4 asks for "one visual system applied uniformly", and a
/// palette assembled colour-by-colour as each widget needed one is how that
/// promise gets broken quietly — nothing is ever *wrong*, the greys just drift
/// apart until the editor looks assembled rather than designed.
///
/// So every neutral here is a step on **one ramp at one hue** — a cool
/// blue-grey near 218° — and every accent is placed against that ramp
/// deliberately. Widgets name a step; they never mix their own.
///
/// The steps are ordered dark to light and each has a job:
///
/// | Step | Job |
/// |---|---|
/// | 00 | the page |
/// | 01 | the cursor's line — one step, felt rather than seen |
/// | 02 | raised surfaces: the status bar |
/// | 03 | borders and rules |
/// | 04 | furniture text: line numbers |
/// | 05 | quiet content: inactive status segments |
/// | 06 | secondary content: file names |
/// | 07 | body text |
/// | 08 | text on a selection |
///
/// Contrast is checked rather than eyeballed — see `typ-core/tests/theme.rs`,
/// which computes WCAG ratios from these channel values and fails the build if a
/// role drops below the floor its ground asks for — 11.5:1 for body text on a
/// dark page, 6.5:1 for content, 5:1 for the gutter. See `audit::Floors` for why
/// a light theme's numbers are lower without being laxer.
mod palette {
    use ratatui::style::Color;

    pub const BASE_00: Color = Color::Rgb(0x10, 0x14, 0x1b);
    pub const BASE_01: Color = Color::Rgb(0x16, 0x1c, 0x25);
    pub const BASE_02: Color = Color::Rgb(0x1a, 0x21, 0x2c);
    pub const BASE_03: Color = Color::Rgb(0x2a, 0x32, 0x40);
    pub const BASE_04: Color = Color::Rgb(0x78, 0x89, 0xa0);
    pub const BASE_05: Color = Color::Rgb(0x84, 0x95, 0xac);
    pub const BASE_06: Color = Color::Rgb(0xa8, 0xb3, 0xc4);
    pub const BASE_07: Color = Color::Rgb(0xcd, 0xd5, 0xe1);
    pub const BASE_08: Color = Color::Rgb(0xe6, 0xec, 0xf5);

    /// The one accent. Focus, links, and anything the eye should be drawn to.
    pub const ACCENT: Color = Color::Rgb(0x68, 0xa6, 0xe4);
    /// The same hue, lifted — directories in the tree.
    pub const ACCENT_BRIGHT: Color = Color::Rgb(0x7f, 0xb3, 0xe0);

    /// Selections sit on the accent's hue at two depths, so the primary reads
    /// as "the same thing, more so" rather than as a different feature.
    pub const SELECT: Color = Color::Rgb(0x25, 0x34, 0x4b);
    pub const SELECT_PRIMARY: Color = Color::Rgb(0x2b, 0x45, 0x6f);

    /// Semantic colours. Deliberately *not* on the base hue: these mean
    /// something, and a reader must not have to decide whether a colour is
    /// decoration or information.
    pub const RED: Color = Color::Rgb(0xec, 0x76, 0x7f);
    pub const AMBER: Color = Color::Rgb(0xfc, 0xd6, 0x90);
    pub const AMBER_DEEP: Color = Color::Rgb(0x3a, 0x35, 0x24);
    pub const TEAL: Color = Color::Rgb(0x56, 0xb6, 0xc2);
}

/// The colors a panel is allowed to know about.
///
/// Deliberately a small copy rather than a reference to a full theme: panels
/// should not be able to reach into application state through their theme.
///
/// Modelled on Helix's `ui.*` scopes, which number 40-plus. This takes the ones
/// TYPE has a use for now or at M3 and no more — but it does take the M3 ones,
/// because a theme file written at M2.5 without diagnostic colours is a theme
/// file that gets a breaking change the moment the LSP client lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub fg: Color,
    pub bg: Color,
    /// The cursor's line. One step off the page — a highlight strong enough to
    /// find deliberately is strong enough to be a stripe across the screen.
    pub cursor_line_bg: Color,

    pub gutter_fg: Color,
    pub gutter_bg: Color,
    pub line_number_fg: Color,
    pub line_number_current_fg: Color,

    /// The dot standing for a space and the arrow standing for a tab, drawn
    /// only where `whitespace` in `config.toml` asks for them.
    ///
    /// The same class of thing as a line number — present, quiet, not content —
    /// so it is held to the same floor and, in every shipped theme, names the
    /// same ramp step. A mark below that floor is texture rather than
    /// information, and a `trailing` setting whose marks cannot be seen catches
    /// nothing.
    pub whitespace: Color,

    /// The vertical rule standing at each completed level of indentation.
    ///
    /// Furniture, like the line numbers and the whitespace marks, and held to
    /// the same floor for the same reason — below it the rules stop being
    /// structure and become a texture down the left of the file. It names the
    /// gutter's step in every shipped theme, which is also what stops the
    /// greys drifting apart one widget at a time.
    pub indent_guide: Color,

    pub selection_bg: Color,
    pub selection_fg: Color,
    /// The primary selection, the one every motion is relative to. Helix themes
    /// this separately for exactly that reason: with thirty cursors, nothing
    /// else says which one is being steered.
    pub selection_primary_bg: Color,

    pub bracket_match_fg: Color,
    pub bracket_match_bg: Color,

    /// The surface chrome sits on: the sidebar, and anything else that frames
    /// the work rather than being it.
    ///
    /// **Distinct from `bg` on purpose.** The tree, the gutter and the editor
    /// were all `bg`, so three regions shared one colour and no amount of
    /// border made them read as separate things. Chrome is raised, content is
    /// the floor — the same two levels the status bar was already using alone.
    ///
    /// The gutter stays on `bg`: it is content, not chrome, and it has its own
    /// reason recorded on `gutter_bg`.
    pub chrome_bg: Color,

    pub border: Color,
    pub border_focused: Color,

    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    /// Segments carrying real but secondary content — filetype, line ending.
    /// Quieter than `status_bar_fg`, never so quiet it stops being readable.
    pub status_bar_inactive_fg: Color,
    pub status_bar_accent: Color,

    pub tree_directory_fg: Color,
    pub tree_file_fg: Color,

    /// Unused until M3. Four lines now against a breaking change to every
    /// shipped theme file later.
    pub diagnostic_error: Color,
    pub diagnostic_warning: Color,
    pub diagnostic_info: Color,
    pub diagnostic_hint: Color,
}

impl Default for ThemeColors {
    fn default() -> Self {
        use palette as p;
        Self {
            fg: p::BASE_07,
            bg: p::BASE_00,
            cursor_line_bg: p::BASE_01,

            gutter_fg: p::BASE_04,
            // The gutter shares the page's background rather than having one of
            // its own: a seam down the left of every file is chrome doing a job
            // the digits already do.
            gutter_bg: p::BASE_00,
            line_number_fg: p::BASE_04,
            // The current line's number matches body text — "here" is stated by
            // being as present as the code, not by being tinted.
            line_number_current_fg: p::BASE_07,

            // The gutter's own step. Whitespace marks and line numbers are the
            // same kind of furniture, and a palette where each widget invents
            // its own grey is how one visual system comes apart.
            whitespace: p::BASE_04,
            indent_guide: p::BASE_04,

            selection_bg: p::SELECT,
            selection_fg: p::BASE_08,
            selection_primary_bg: p::SELECT_PRIMARY,

            bracket_match_fg: p::AMBER,
            bracket_match_bg: p::AMBER_DEEP,

            // The same step the status bar uses. Chrome is one surface, not a
            // ladder of them: a third level would have to be `base01`, which is
            // the cursor-line tint, and a sidebar the exact colour of the
            // current-line stripe is a collision waiting to confuse.
            chrome_bg: p::BASE_02,

            border: p::BASE_03,
            border_focused: p::ACCENT,

            status_bar_bg: p::BASE_02,
            status_bar_fg: p::BASE_07,
            status_bar_inactive_fg: p::BASE_05,
            status_bar_accent: p::ACCENT,

            tree_directory_fg: p::ACCENT_BRIGHT,
            tree_file_fg: p::BASE_06,

            diagnostic_error: p::RED,
            diagnostic_warning: p::AMBER,
            diagnostic_info: p::ACCENT,
            diagnostic_hint: p::TEAL,
        }
    }
}

/// Everything a panel may see at render time.
///
/// This is the whole surface — a panel never receives `&AppState`.
pub struct RenderContext<'a> {
    pub theme: &'a ThemeColors,
    /// Syntax capture styles, already degraded to the terminal's colour depth.
    ///
    /// Beside `theme` rather than inside it because `ThemeColors` is `Copy` and
    /// a `BTreeMap` would end that. Both halves of the theme travel with the
    /// frame, which is what keeps a theme switch from updating the palette and
    /// leaving the syntax colours behind — Helix passes its whole `Theme` into
    /// every render call for the same reason, and Zed reaches both halves
    /// through one accessor.
    pub syntax: &'a crate::SyntaxTheme,
    /// What the language servers have said about the buffer being drawn.
    ///
    /// **Here rather than through a setter**, for the reason the theme's two
    /// halves taught: a setter is the smaller diff and leaves the other half
    /// stale on a tab switch. Diagnostics belong to a document, the frame knows
    /// which document it is drawing, so they travel with the frame.
    pub diagnostics: &'a [crate::Diagnostic],
    pub is_focused: bool,
    pub panel_index: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
}

/// A rectangular, focusable unit of UI.
///
/// Implementors provide five methods; everything else has a default. Panels
/// communicate outward by returning events, never by mutating shared state.
pub trait Panel: Any {
    /// Stable type name, used for registry lookup and session records.
    fn name(&self) -> &'static str;

    /// Dynamic title shown in the panel header.
    fn title(&self) -> String;

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext);

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent>;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// `panel_area` is supplied so the panel can translate to local coordinates.
    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        let _ = (event, panel_area);
        Vec::new()
    }

    /// Coalesced scroll. Positive is down.
    fn handle_scroll(&mut self, delta: i32, panel_area: Rect) -> Vec<PanelEvent> {
        let _ = (delta, panel_area);
        Vec::new()
    }

    /// Where the terminal cursor belongs, in screen coordinates, when this
    /// panel holds focus. `None` hides it.
    ///
    /// The app draws the real terminal cursor rather than a styled cell, so it
    /// blinks and reshapes the way every other terminal program's does. A panel
    /// with nothing to edit — a file tree, a viewer — leaves this defaulted.
    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        let _ = panel_area;
        None
    }

    /// Perform a named action.
    ///
    /// This is the only way a binding, the command palette, or the vim layer
    /// reaches a panel's behavior.
    ///
    /// `None` means "I do not handle this action" and lets the app try it.
    /// `Some(vec![])` means "handled, nothing to report" — a real outcome, as
    /// when adding a cursor at the edge of the document does nothing. Folding
    /// those two answers into an empty vector reads fine today and becomes a
    /// silent bug the first time an action needs both a panel implementation
    /// and an app fallback.
    fn apply_action(&mut self, action: crate::Action) -> Option<Vec<PanelEvent>> {
        let _ = action;
        None
    }

    /// Periodic hook for background work.
    fn tick(&mut self) -> Vec<PanelEvent> {
        Vec::new()
    }

    /// True when the panel consumes Escape itself (e.g. an open search box).
    fn captures_escape(&self) -> bool {
        false
    }

    /// `Some(message)` blocks closing until confirmed.
    fn needs_close_confirmation(&self) -> Option<String> {
        None
    }
}
