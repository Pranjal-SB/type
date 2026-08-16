//! The gutter, as an ordered list of components rather than a line-number column.
//!
//! Helix's `helix-view/src/gutter.rs` does not draw line numbers; it draws
//! `GutterType::{LineNumbers, Diagnostics, Diff, Spacer, CodeActionHint}`, each
//! with a width and a renderer, in configurable order. That shape is taken here
//! for a specific reason: diagnostics arrive at M3 and git-diff markers at M5,
//! and both want this column. A hardcoded line-number gutter would land the
//! feature and lose the design, and the second component is what forces the
//! rewrite.
//!
//! So `Diagnostics` and `Diff` exist today, reserve their cell, and draw
//! nothing. M3 and M5 fill in a function instead of re-laying-out the editor.

use ratatui::style::Style;
use ratatui::text::Span;
use typ_core::ThemeColors;

/// Digits needed to write the largest line number in a buffer of `line_count`
/// lines. Never zero: an empty buffer still shows line 1.
fn digits(line_count: usize) -> usize {
    let mut n = line_count.max(1);
    let mut digits = 1;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits
}

/// One column, or group of columns, in the gutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutterComponent {
    /// Line numbers, as wide as the buffer's largest.
    ///
    /// `relative` numbers every line by its distance from the cursor, which is
    /// a modal-editing idiom. TYPE is non-modal by default so it ships off —
    /// but the field exists rather than the variant being split in two, so the
    /// vim layer flips a bool instead of replacing the component.
    LineNumbers { relative: bool },
    /// Blank separation, so digits do not sit flush against the text.
    Spacer,
    /// Error and warning markers. **M3.** Reserves its cell and draws nothing.
    Diagnostics,
    /// Added/removed/changed markers from git. **M5.** Same.
    Diff,
}

impl GutterComponent {
    /// Cells this component occupies. Constant per frame — the gutter's width
    /// must not change as the view scrolls, or the text shifts sideways when
    /// the viewport reaches line 100.
    pub fn width(&self, line_count: usize) -> usize {
        match self {
            GutterComponent::LineNumbers { .. } => digits(line_count),
            GutterComponent::Spacer | GutterComponent::Diagnostics | GutterComponent::Diff => 1,
        }
    }

    fn render_line(
        &self,
        line: usize,
        cursor_line: usize,
        line_count: usize,
        theme: &ThemeColors,
    ) -> Span<'static> {
        match self {
            GutterComponent::LineNumbers { relative } => {
                let number = if *relative && line != cursor_line {
                    cursor_line.abs_diff(line)
                } else {
                    // 1-based, matching every compiler error and every other
                    // editor. Under relative numbering the cursor's own line
                    // keeps its absolute number, which is what makes the pair
                    // useful together.
                    line + 1
                };
                let width = digits(line_count);
                let style = if line == cursor_line {
                    Style::default().fg(theme.fg)
                } else {
                    Style::default().fg(theme.line_numbers)
                };
                // Right-aligned, so the text edge stays straight as the numbers
                // grow a digit.
                Span::styled(format!("{number:>width$}"), style)
            }
            GutterComponent::Spacer | GutterComponent::Diagnostics | GutterComponent::Diff => {
                Span::raw(" ")
            }
        }
    }
}

/// The gutter: components in draw order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gutter {
    components: Vec<GutterComponent>,
}

impl Default for Gutter {
    fn default() -> Self {
        Self::new(vec![
            GutterComponent::LineNumbers { relative: false },
            GutterComponent::Spacer,
        ])
    }
}

impl Gutter {
    pub fn new(components: Vec<GutterComponent>) -> Self {
        Self { components }
    }

    /// Total cells, summed across components. Zero for an empty gutter, which
    /// is how the column is turned off without a second code path.
    pub fn width(&self, line_count: usize) -> usize {
        self.components.iter().map(|c| c.width(line_count)).sum()
    }

    /// The spans for one buffer line.
    pub fn render_line(
        &self,
        line: usize,
        cursor_line: usize,
        line_count: usize,
        theme: &ThemeColors,
    ) -> Vec<Span<'static>> {
        self.components
            .iter()
            .map(|c| c.render_line(line, cursor_line, line_count, theme))
            .collect()
    }
}
