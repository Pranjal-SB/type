//! Turning a line of text plus the selections covering it into styled spans.
//!
//! Split out of `lib.rs` because this is where display-column arithmetic
//! lives, and it is the part most likely to grow: highlighting arrived in M2.7
//! and had to compose with selection styling rather than fight it.

use std::ops::Range;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use typ_buffer::{Position, Selection, display_width_with_tabs};
use typ_core::ThemeColors;
use unicode_segmentation::UnicodeSegmentation;

/// Drop `left_col` display columns from the front of a line.
///
/// Returns the remaining text, how many graphemes were dropped, and the display
/// column the remainder starts at. The grapheme count is what lines the result
/// up against selections, which are stated in grapheme columns; the display
/// column is what lines it up against the indent guides, which are not.
///
/// A wide grapheme straddling the boundary is dropped entirely rather than
/// half-drawn: a terminal cannot render half a cell, so the alternatives are a
/// dropped character or a row one column out of alignment with every other row.
/// Slicing by display column rather than by grapheme is the whole point — a line
/// of CJK scrolls by cells the way it is drawn.
pub fn window(text: &str, left_col: usize, tab_width: usize) -> (&str, usize, usize) {
    if left_col == 0 {
        return (text, 0, 0);
    }

    let mut column = 0usize;
    for (skipped, (byte, grapheme)) in text.grapheme_indices(true).enumerate() {
        if column >= left_col {
            return (&text[byte..], skipped, column);
        }
        // `.max(1)` so a zero-width grapheme cannot stall the walk. Tabs are
        // measured from their real column, which is why this tracks `column`
        // rather than summing widths in isolation.
        column += if grapheme == "\t" {
            tab_width - (column % tab_width)
        } else {
            display_width_with_tabs(grapheme, tab_width).max(1)
        };
    }
    // Scrolled entirely past the end of this line.
    ("", text.graphemes(true).count(), column)
}

/// Which whitespace gets a visible mark.
///
/// VS Code's set minus `boundary` — "every run except a single space between
/// words" needs word segmentation inside the render loop for the least useful of
/// the five. Fresh's finer leading/inner/trailing split is a superset of
/// [`Whitespace::Trailing`] and can arrive later without breaking a file: the
/// setting is a string and adding a value is additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Whitespace {
    None,
    /// Whitespace after the last non-whitespace grapheme on the line — the only
    /// value here that catches a defect rather than answering curiosity.
    Trailing,
    /// Inside a selection, which is where it is diagnostic and nowhere else.
    /// VS Code's default, and the reason is that it costs nothing when you are
    /// not asking and is exact when you are.
    #[default]
    Selection,
    All,
}

/// A foreground laid over whatever ground the cell already had.
///
/// **Not [`Paint`] variants.** `Paint` chooses the background; these choose
/// only the foreground, so the two compose — a marked space inside a selection
/// keeps the selection's ground and takes its foreground from `whitespace`. A
/// run has to break when either changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    /// A whitespace mark, where the setting asked for one.
    Mark,
    /// An indent guide.
    Guide,
    /// A tree-sitter capture, as an index into [`LineStyle::syntax`].
    ///
    /// An index rather than a `Style` so `Overlay` stays `Copy` and the
    /// run-break test stays an integer compare. The loop runs once per cell on
    /// the keystroke path.
    Syntax(u32),
}

/// The glyph a whitespace grapheme is drawn as.
fn mark_for(grapheme: &str) -> Option<char> {
    match grapheme {
        " " => Some('·'),
        "\t" => Some('→'),
        _ => None,
    }
}

/// What a cell is painted as. Ordered by precedence, highest last.
///
/// Spelling this out as a type rather than as a chain of `if`s inside the loop
/// is what makes the precedence reviewable: there is exactly one place that
/// decides.
///
/// This comment used to predict that syntax highlighting would add a variant
/// *here*. The instinct was right and the placement was wrong: M2.5 split the
/// per-cell style into two axes this comment predates, and a capture sets a
/// foreground. It is an [`Overlay`]. Had it landed here, every scope would
/// multiply with every selection state, and the first thing to break would be
/// the one that already worked — a selected keyword keeping the selection's
/// background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Paint {
    Plain,
    /// The line a caret sits on.
    CursorLine,
    /// A bracket and its partner.
    Bracket,
    /// One of the non-primary selections.
    Selection,
    /// The selection every motion is relative to.
    PrimarySelection,
}

impl Paint {
    fn style(self, theme: &ThemeColors) -> Style {
        match self {
            Paint::Plain => Style::default().fg(theme.fg).bg(theme.bg),
            Paint::CursorLine => Style::default().fg(theme.fg).bg(theme.cursor_line_bg),
            Paint::Bracket => Style::default()
                .fg(theme.bracket_match_fg)
                .bg(theme.bracket_match_bg),
            Paint::Selection => Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_bg),
            Paint::PrimarySelection => Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_primary_bg),
        }
    }
}

/// Everything needed to draw one visible line.
///
/// A struct rather than nine positional arguments: the call site was already at
/// six and the three effects added here would have made it a row of unlabelled
/// values where swapping two `usize`s compiles cleanly and renders wrong.
pub struct LineStyle<'a> {
    pub line: usize,
    pub left_col: usize,
    /// Text-area width in cells, for padding the current-line highlight out to
    /// the edge.
    pub width: usize,
    pub tab_width: usize,
    pub selections: &'a [Selection],
    pub primary: Selection,
    /// Whether a caret sits on this line *with nothing selected*. A line
    /// carrying a real selection does not also get the stripe — the selection
    /// is already saying where the user is, and two answers to one question is
    /// how a interface starts to look busy.
    pub cursor_line: bool,
    pub brackets: Option<(Position, Position)>,
    pub whitespace: Whitespace,
    /// Completed levels of indent this line draws a guide for, one guide per
    /// level starting at column zero.
    ///
    /// Levels rather than columns, because two spaces in a file that indents in
    /// fours is alignment and not nesting — a rule through it would stand in
    /// the middle of every wrapped argument list. The caller owns this because
    /// a blank line's depth comes from its neighbours, and the panel is the
    /// only thing here that can see them.
    pub indent_guides: usize,
    /// Resolved syntax styles for this line, ascending and non-overlapping, in
    /// **grapheme columns of the whole line**.
    ///
    /// Columns, not bytes: invariant 4 says `col` is a grapheme index, and a
    /// render loop taking byte offsets is how that gets broken quietly on the
    /// first non-ASCII file. The panel converts, once per line, before this
    /// slice exists.
    ///
    /// Styles rather than scope names, because resolving a name against the
    /// theme is a `BTreeMap` walk and this runs once per cell.
    pub syntax: &'a [(Range<usize>, Style)],
    pub theme: &'a ThemeColors,
}

/// Build one rendered line, splitting it into spans wherever the paint changes.
///
/// Spans are cut at grapheme boundaries and styled per run, so a wide
/// character is highlighted as one unit and never half-painted.
pub fn styled_line(text: &str, ctx: &LineStyle) -> Line<'static> {
    // Selections are stated in grapheme columns of the whole line, so the
    // dropped count is what keeps highlighting on the text it covers rather
    // than sliding left with the window. The display column is what keeps the
    // guides standing over their real tab stops on a line scrolled sideways.
    let (visible, skipped, start) = window(text, ctx.left_col, ctx.tab_width);

    // Where trailing whitespace starts, as a byte offset into `visible`.
    // `window` returns a suffix, so trimming what is drawn finds the same tail
    // the whole line has. Once per line rather than once per cell, and only for
    // the one setting that asks the question.
    let tail = match ctx.whitespace {
        Whitespace::Trailing => visible.trim_end_matches([' ', '\t']).len(),
        _ => usize::MAX,
    };

    // One past the last column a guide can stand in. Computed once, so the
    // per-cell test is a comparison and a remainder rather than a division.
    let guides_end = ctx.indent_guides * ctx.tab_width;

    let mut runs = Runs::default();
    let mut column = start;
    // Walks forward with the loop rather than searching per cell: the spans
    // are ascending and non-overlapping, so this is one pass over both
    // sequences instead of a scan of the line's spans for every grapheme.
    let mut span = 0usize;

    for (offset, (byte, grapheme)) in visible.grapheme_indices(true).enumerate() {
        let position = Position {
            line: ctx.line,
            col: skipped + offset,
        };
        let paint = paint_for(position, ctx);
        while span < ctx.syntax.len() && ctx.syntax[span].0.end <= position.col {
            span += 1;
        }
        let scope = match ctx.syntax.get(span) {
            Some((range, _)) if range.contains(&position.col) => Overlay::Syntax(span as u32),
            _ => Overlay::None,
        };
        // The *original* grapheme's width, always. A tab drawn as one arrow
        // still occupies its whole run of columns; anything else loses three of
        // them and leaves every glyph after it out of step with a cursor that
        // is still counting the tab as four.
        let width = display_width_with_tabs(grapheme, ctx.tab_width).max(1);
        let mark = mark_for(grapheme).filter(|_| marks(ctx.whitespace, paint, byte, tail));

        match mark {
            // A mark wins its own cell. It is on screen because the user asked
            // to see that character; a guide is ambient, and the one that can
            // be switched off is not the one to overdraw.
            Some(glyph) => {
                runs.push(glyph, (paint, Overlay::Mark), ctx);
                // Then blank to the end of what the grapheme occupied.
                for cell in 1..width {
                    runs.blank(column + cell, guides_end, paint, ctx);
                }
            }
            // Indentation is where guides live, so its cells are emitted one at
            // a time. An unmarked tab is expanded too — it has to be: otherwise
            // a tab would draw in one column with marks off and four with them
            // on, and selecting a line would shift its text three columns left.
            None if grapheme == " " || grapheme == "\t" => {
                for cell in 0..width {
                    runs.blank(column + cell, guides_end, paint, ctx);
                }
            }
            // Text. The only branch a capture can reach: the two above are a
            // mark and indentation, which is the whole of "mark > guide >
            // syntax" — no grammar captures a space, and a guide stands only
            // where indentation is.
            None => runs.push_str(grapheme, (paint, scope), ctx),
        }
        column += width;
    }

    // A blank line has no cells of its own for its guides to stand in, and it
    // is the line that most needs them: without this, every empty line inside a
    // block punches a hole through the run.
    let ground = if ctx.cursor_line {
        Paint::CursorLine
    } else {
        Paint::Plain
    };
    while column < guides_end && column - start < ctx.width {
        runs.blank(column, guides_end, ground, ctx);
        column += 1;
    }

    let mut spans = runs.finish(ctx);

    // Carry the current-line tint past the end of the text. A highlight that
    // stops at the last character leaves a ragged right edge that reads as a
    // rendering bug rather than as a feature.
    let drawn = column - start;
    if ctx.cursor_line && drawn < ctx.width {
        spans.push(Span::styled(
            " ".repeat(ctx.width - drawn),
            Paint::CursorLine.style(ctx.theme),
        ));
    }

    Line::from(spans)
}

/// Spans under construction, cut wherever the paint or the overlay changes.
///
/// A type rather than three locals, because the loop above emits at two
/// granularities — a whole grapheme for text, one cell at a time for the
/// whitespace a guide can stand in — and both have to break runs the same way.
#[derive(Default)]
struct Runs {
    spans: Vec<Span<'static>>,
    current: String,
    run: Option<(Paint, Overlay)>,
}

impl Runs {
    fn open(&mut self, run: (Paint, Overlay), ctx: &LineStyle) {
        if self.run != Some(run) && !self.current.is_empty() {
            let style = style_of(self.run.unwrap_or((Paint::Plain, Overlay::None)), ctx);
            self.spans
                .push(Span::styled(std::mem::take(&mut self.current), style));
        }
        self.run = Some(run);
    }

    fn push(&mut self, ch: char, run: (Paint, Overlay), ctx: &LineStyle) {
        self.open(run, ctx);
        self.current.push(ch);
    }

    fn push_str(&mut self, s: &str, run: (Paint, Overlay), ctx: &LineStyle) {
        self.open(run, ctx);
        self.current.push_str(s);
    }

    /// One blank cell, or the guide standing in it.
    fn blank(&mut self, column: usize, guides_end: usize, paint: Paint, ctx: &LineStyle) {
        if column < guides_end && column.is_multiple_of(ctx.tab_width) {
            self.push('│', (paint, Overlay::Guide), ctx);
        } else {
            self.push(' ', (paint, Overlay::None), ctx);
        }
    }

    fn finish(mut self, ctx: &LineStyle) -> Vec<Span<'static>> {
        if !self.current.is_empty() {
            let style = style_of(self.run.unwrap_or((Paint::Plain, Overlay::None)), ctx);
            self.spans.push(Span::styled(self.current, style));
        }
        self.spans
    }
}

/// A run's style: its paint, with any overlay laid over the foreground.
///
/// Takes the whole `LineStyle` rather than just the theme because a syntax
/// overlay carries an index into this line's resolved spans, and only the
/// caller's context can turn that back into a colour.
fn style_of((paint, overlay): (Paint, Overlay), ctx: &LineStyle) -> Style {
    let style = paint.style(ctx.theme);
    match overlay {
        Overlay::None => style,
        Overlay::Mark => style.fg(ctx.theme.whitespace),
        Overlay::Guide => style.fg(ctx.theme.indent_guide),
        Overlay::Syntax(i) => match ctx.syntax.get(i as usize) {
            Some((_, syntax)) => {
                let mut out = style;
                if let Some(fg) = syntax.fg {
                    out = out.fg(fg);
                }
                // A theme may put a background on a scope, and it gets one only
                // where nothing more urgent has claimed the ground. A selected
                // keyword keeps the selection's background — that is the whole
                // reason a capture is an overlay rather than a paint.
                if let Some(bg) = syntax.bg
                    && matches!(paint, Paint::Plain | Paint::CursorLine)
                {
                    out = out.bg(bg);
                }
                // Bold keywords, italic comments. `Paint` sets no modifiers, so
                // there is nothing here to conflict with.
                out.add_modifier(syntax.add_modifier)
            }
            None => style,
        },
    }
}

/// Whether this cell's whitespace is whitespace the user asked to see.
///
/// `Selection` reads the paint rather than scanning the selections again —
/// `paint_for` has already answered exactly that question, and asking it twice
/// per cell is a second pass over every selection on the keystroke path.
fn marks(setting: Whitespace, paint: Paint, byte: usize, tail: usize) -> bool {
    match setting {
        Whitespace::None => false,
        Whitespace::All => true,
        Whitespace::Trailing => byte >= tail,
        Whitespace::Selection => matches!(paint, Paint::Selection | Paint::PrimarySelection),
    }
}

fn paint_for(position: Position, ctx: &LineStyle) -> Paint {
    if ctx.selections.iter().any(|s| s.contains(position)) {
        // A selection outranks a bracket: both mean "where you are", and the
        // selection is the one the next keystroke acts on.
        if ctx.primary.contains(position) {
            Paint::PrimarySelection
        } else {
            Paint::Selection
        }
    } else if ctx
        .brackets
        .is_some_and(|(open, close)| open == position || close == position)
    {
        Paint::Bracket
    } else if ctx.cursor_line {
        Paint::CursorLine
    } else {
        Paint::Plain
    }
}
