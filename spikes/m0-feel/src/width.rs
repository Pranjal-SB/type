use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Display columns occupied by a single grapheme cluster.
///
/// Tabs are handled by callers that know the current column, so this reports
/// a tab as 0 and lets them add the tab-stop padding.
fn grapheme_width(g: &str) -> usize {
    if g == "\t" {
        0
    } else {
        // Zero-width and combining sequences report 0 here, which is correct.
        UnicodeWidthStr::width(g)
    }
}

/// Total display columns a string occupies, expanding tabs to `tab_width` stops.
pub fn display_width_with_tabs(s: &str, tab_width: usize) -> usize {
    let mut col = 0usize;
    for g in s.graphemes(true) {
        if g == "\t" {
            col += tab_width - (col % tab_width);
        } else {
            col += grapheme_width(g);
        }
    }
    col
}

/// Total display columns, using the default tab width of 4.
pub fn display_width(s: &str) -> usize {
    display_width_with_tabs(s, 4)
}

/// Display column at which the grapheme at `grapheme_idx` begins.
pub fn grapheme_to_display_col(line: &str, grapheme_idx: usize, tab_width: usize) -> usize {
    let mut col = 0usize;
    for (i, g) in line.graphemes(true).enumerate() {
        if i == grapheme_idx {
            return col;
        }
        if g == "\t" {
            col += tab_width - (col % tab_width);
        } else {
            col += grapheme_width(g);
        }
    }
    col
}

/// Grapheme index containing `display_col`.
///
/// Clicking anywhere inside a wide grapheme selects that grapheme, so the
/// right half of a CJK character does not land on the following one. Clicks
/// past the end of the line clamp to the line length.
pub fn display_to_grapheme_col(line: &str, display_col: usize, tab_width: usize) -> usize {
    let mut col = 0usize;
    for (i, g) in line.graphemes(true).enumerate() {
        let w = if g == "\t" {
            tab_width - (col % tab_width)
        } else {
            grapheme_width(g)
        };
        if display_col < col + w.max(1) {
            return i;
        }
        col += w;
    }
    line.graphemes(true).count()
}
