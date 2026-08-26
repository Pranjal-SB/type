//! The strip of open files above the editor.
//!
//! Laid out by [`cells`] and drawn by [`draw`], in that order and never the
//! other way round: Task 6's hit-testing asks [`cells`] the same question the
//! renderer asks it, so a click resolves to the tab under the pointer by
//! construction rather than by two pieces of arithmetic agreeing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_buffer::display_width;
use typ_core::ThemeColors;
use unicode_segmentation::UnicodeSegmentation;

/// A space either side of the name, so adjacent tabs do not read as one word,
/// plus the column the close box sits in.
const PADDING: u16 = 3;

/// The close box. On every tab rather than only the active one: a terminal has
/// no hover, so the GUI trick of revealing it under the pointer has nothing to
/// reveal it with, and sizing only the active cell for it makes the whole bar
/// jiggle every time the active tab changes.
const CLOSE: &str = "×";

/// Where one tab sits in the bar.
///
/// `width` is what the cell was given, which for the last visible cell can be
/// less than the name wants — clipped rather than dropped, because a bar that
/// silently omits the tab you are editing is worse than a truncated name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabCell {
    /// Index into the app's tab list. Not the position in the returned slice:
    /// the bar scrolls, so the first visible cell is often not tab zero.
    pub index: usize,
    /// Column offset from the bar's left edge.
    pub x: u16,
    pub width: u16,
}

/// Columns a label wants, padding included.
fn cell_width(label: &str) -> u16 {
    let text = u16::try_from(display_width(label)).unwrap_or(u16::MAX);
    text.saturating_add(PADDING)
}

/// The column holding this cell's close box, if it has one.
///
/// `None` when the bar's edge clipped the cell, because then that column is
/// part of the filename. Closing a file by clicking the last letter of a name
/// that happened to run long is not an accident anyone should be able to have.
pub fn close_box_x(cell: &TabCell, label: &str) -> Option<u16> {
    (cell.width == cell_width(label)).then(|| cell.x + cell.width - 1)
}

/// The visible cells, scrolled so `active` is one of them.
///
/// Stateless: the window is recomputed from `active` on every call rather than
/// remembered. A remembered offset is a second source of truth about which tabs
/// are on screen, and it goes stale the moment a tab is closed.
pub fn cells(labels: &[String], active: usize, width: u16) -> Vec<TabCell> {
    if labels.is_empty() || width == 0 {
        return Vec::new();
    }
    let widths: Vec<u16> = labels.iter().map(|l| cell_width(l)).collect();
    let start = first_visible(&widths, active.min(labels.len() - 1), width);

    let mut cells = Vec::new();
    let mut x = 0u16;
    for (index, wanted) in widths.iter().enumerate().skip(start) {
        if x >= width {
            break;
        }
        cells.push(TabCell {
            index,
            x,
            width: (*wanted).min(width - x),
        });
        x = x.saturating_add(*wanted);
    }
    cells
}

/// The leftmost tab that can be first while `active` still gets a column.
///
/// Scrolls by the smallest amount that keeps the promise, so moving one tab to
/// the right shifts the bar by one tab rather than paging it.
fn first_visible(widths: &[u16], active: usize, width: u16) -> usize {
    for start in 0..=active {
        let mut x = 0u16;
        for (index, wanted) in widths.iter().enumerate().skip(start) {
            if x >= width {
                break;
            }
            if index == active {
                return start;
            }
            x = x.saturating_add(*wanted);
        }
    }
    active
}

/// Draw the bar. `labels` carries the dirty marker already, so there is one
/// spelling of "unsaved" in the editor rather than two.
pub fn draw(buf: &mut Buffer, area: Rect, labels: &[String], active: usize, theme: &ThemeColors) {
    if area.height == 0 {
        return;
    }
    // No new theme fields. The bar is chrome, so it takes the chrome background
    // and the status bar's dimmed foreground; the **active** tab takes the
    // editor's own `fg` on `bg`, which is what visually joins it to the pane
    // underneath — the tab and the text it names are painted the same.
    let inactive = Style::default()
        .fg(theme.status_bar_inactive_fg)
        .bg(theme.chrome_bg);
    let selected = Style::default()
        .fg(theme.fg)
        .bg(theme.bg)
        .add_modifier(Modifier::BOLD);

    // The whole row first: without it the cells sit on whatever the previous
    // frame left in the gap after the last tab.
    for x in area.x..area.right() {
        buf[(x, area.y)].set_symbol(" ").set_style(inactive);
    }

    for cell in cells(labels, active, area.width) {
        let style = if cell.index == active {
            selected
        } else {
            inactive
        };
        write_cell(buf, area, cell, &labels[cell.index], style);
    }
}

/// One cell: a leading space, as much of the name as fits, a trailing space.
fn write_cell(buf: &mut Buffer, area: Rect, cell: TabCell, label: &str, style: Style) {
    let mut x = area.x + cell.x;
    let end = (area.x + cell.x + cell.width).min(area.right());

    let mut put = |x: &mut u16, symbol: &str| {
        if *x < end {
            buf[(*x, area.y)].set_symbol(symbol).set_style(style);
            // A double-width grapheme owns the next cell too, and leaving that
            // cell's old symbol behind is how a CJK name grows a stray glyph.
            let columns = u16::try_from(display_width(symbol)).unwrap_or(1).max(1);
            for trailing in 1..columns {
                if *x + trailing < end {
                    buf[(*x + trailing, area.y)].set_symbol("").set_style(style);
                }
            }
            *x = x.saturating_add(columns);
        }
    };

    put(&mut x, " ");
    for grapheme in label.graphemes(true) {
        // Leave room for the trailing space and the close box, so a long name
        // is clipped rather than growing over them.
        if x + 2 >= end {
            break;
        }
        put(&mut x, grapheme);
    }
    put(&mut x, " ");
    put(&mut x, CLOSE);
}
