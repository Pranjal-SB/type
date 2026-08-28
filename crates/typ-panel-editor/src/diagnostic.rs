//! Diagnostics, arranged for the rows about to be drawn.
//!
//! **`for_viewport`, not `for_buffer`**, and the same argument `highlight.rs`
//! makes: a 50k-line file with four hundred diagnostics must not walk them all
//! to paint forty rows, and it must not walk them *per row* either. One pass
//! over the diagnostics per frame, bucketed by line, and the render loop then
//! walks each line's ranges forward alongside the graphemes — the way it
//! already walks the syntax spans.

use std::ops::Range;

use typ_core::{Diagnostic, Severity};

/// What one visible line has to draw.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LineDiagnostics {
    /// The worst severity anywhere on the line, for the gutter.
    ///
    /// `Severity` is ordered worst-first, so this is a running minimum.
    pub worst: Option<Severity>,
    /// Ranges to underline, in **grapheme columns of the whole line**,
    /// ascending and non-overlapping. The units `LineStyle::syntax` uses, for
    /// the same reason: invariant 4, and a render loop taking byte offsets is
    /// how it gets broken quietly on the first non-ASCII file.
    pub ranges: Vec<(Range<usize>, Severity)>,
}

/// A range that reaches the end of whatever line it is on.
///
/// A diagnostic spanning three lines covers all of the middle one, and this
/// module cannot see how long that line is. The render loop stops at the text,
/// so an end past it costs nothing.
const TO_END: usize = usize::MAX;

/// Bucket `diagnostics` into the rows `lines` will draw.
///
/// Everything outside the viewport is skipped rather than clipped, so the cost
/// is one comparison per diagnostic and nothing else.
pub fn for_viewport(diagnostics: &[Diagnostic], lines: Range<usize>) -> Vec<LineDiagnostics> {
    let mut rows = vec![LineDiagnostics::default(); lines.len()];
    if lines.is_empty() {
        return rows;
    }

    for diagnostic in diagnostics {
        let (start, end) = diagnostic.range;
        if end.line < lines.start || start.line >= lines.end {
            continue;
        }
        let first = start.line.max(lines.start);
        let last = end.line.min(lines.end - 1);

        for line in first..=last {
            let from = if line == start.line { start.col } else { 0 };
            let to = if line == end.line { end.col } else { TO_END };
            // A server marks a missing token with an empty range — a semicolon
            // that should be there and is not. Underlining nothing says
            // nothing, so it takes the cell it points at.
            let to = if to <= from { from + 1 } else { to };

            let row = &mut rows[line - lines.start];
            row.worst = Some(match row.worst {
                Some(worst) => worst.min(diagnostic.severity),
                None => diagnostic.severity,
            });
            row.ranges.push((from..to, diagnostic.severity));
        }
    }

    for row in &mut rows {
        flatten(&mut row.ranges);
    }
    rows
}

/// Make one line's ranges ascending and non-overlapping, worst severity
/// winning where they cross.
///
/// The render loop walks these forward alongside the graphemes and never
/// searches, which is only correct if they do not overlap. Two servers
/// reporting the same token — or one server reporting an error inside the span
/// of a warning — is the ordinary case rather than the strange one.
fn flatten(ranges: &mut Vec<(Range<usize>, Severity)>) {
    if ranges.len() < 2 {
        ranges.sort_by_key(|(range, _)| range.start);
        return;
    }

    // Every column any range starts or ends at. The segments between them are
    // exactly the pieces whose severity is constant.
    let mut edges: Vec<usize> = ranges
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();

    let mut flat: Vec<(Range<usize>, Severity)> = Vec::with_capacity(edges.len());
    for pair in edges.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let worst = ranges
            .iter()
            .filter(|(range, _)| range.start <= from && to <= range.end)
            .map(|(_, severity)| *severity)
            .min();
        let Some(worst) = worst else { continue };
        // Join a segment to the one before it when nothing changed, so a run
        // of equal severity is one range rather than one per edge.
        match flat.last_mut() {
            Some((last, previous)) if last.end == from && *previous == worst => last.end = to,
            _ => flat.push((from..to, worst)),
        }
    }
    *ranges = flat;
}

#[cfg(test)]
mod tests {
    use super::*;
    use typ_buffer::Position;

    fn at(line: usize, col: usize) -> Position {
        Position { line, col }
    }

    fn diagnostic(severity: Severity, from: Position, to: Position) -> Diagnostic {
        Diagnostic {
            range: (from, to),
            severity,
            message: String::new(),
            source: None,
        }
    }

    #[test]
    fn a_diagnostic_lands_on_its_own_line() {
        let rows = for_viewport(&[diagnostic(Severity::Error, at(1, 2), at(1, 5))], 0..3);
        assert_eq!(rows[0], LineDiagnostics::default());
        assert_eq!(rows[1].worst, Some(Severity::Error));
        assert_eq!(rows[1].ranges, vec![(2..5, Severity::Error)]);
        assert_eq!(rows[2], LineDiagnostics::default());
    }

    #[test]
    fn one_outside_the_viewport_costs_a_comparison_and_nothing_else() {
        let rows = for_viewport(&[diagnostic(Severity::Error, at(90, 0), at(90, 4))], 0..3);
        assert!(rows.iter().all(|row| row.worst.is_none()));
    }

    #[test]
    fn a_span_across_lines_covers_the_middle_ones_entirely() {
        let rows = for_viewport(&[diagnostic(Severity::Error, at(0, 3), at(2, 1))], 0..3);
        assert_eq!(rows[0].ranges, vec![(3..TO_END, Severity::Error)]);
        assert_eq!(rows[1].ranges, vec![(0..TO_END, Severity::Error)]);
        assert_eq!(rows[2].ranges, vec![(0..1, Severity::Error)]);
    }

    #[test]
    fn a_span_starting_above_the_viewport_is_clipped_to_it() {
        let rows = for_viewport(&[diagnostic(Severity::Error, at(0, 3), at(9, 1))], 4..6);
        assert_eq!(rows[0].ranges, vec![(0..TO_END, Severity::Error)]);
        assert_eq!(rows[1].ranges, vec![(0..TO_END, Severity::Error)]);
    }

    #[test]
    fn an_empty_range_takes_the_cell_it_points_at() {
        let rows = for_viewport(&[diagnostic(Severity::Error, at(0, 9), at(0, 9))], 0..1);
        assert_eq!(rows[0].ranges, vec![(9..10, Severity::Error)]);
    }

    #[test]
    fn the_worst_severity_on_a_line_is_the_one_the_gutter_gets() {
        let rows = for_viewport(
            &[
                diagnostic(Severity::Hint, at(0, 0), at(0, 1)),
                diagnostic(Severity::Error, at(0, 4), at(0, 5)),
                diagnostic(Severity::Warning, at(0, 8), at(0, 9)),
            ],
            0..1,
        );
        assert_eq!(rows[0].worst, Some(Severity::Error));
    }

    #[test]
    fn overlapping_ranges_are_split_with_the_worst_winning() {
        let rows = for_viewport(
            &[
                diagnostic(Severity::Warning, at(0, 0), at(0, 10)),
                diagnostic(Severity::Error, at(0, 4), at(0, 6)),
            ],
            0..1,
        );
        assert_eq!(
            rows[0].ranges,
            vec![
                (0..4, Severity::Warning),
                (4..6, Severity::Error),
                (6..10, Severity::Warning),
            ]
        );
    }

    #[test]
    fn identical_ranges_collapse_rather_than_repeating() {
        // Two servers on one document, or cargo and rust-analyzer both naming
        // the same token. The render loop walks these forward and never
        // searches, which is only correct if they do not overlap.
        let rows = for_viewport(
            &[
                diagnostic(Severity::Error, at(0, 2), at(0, 6)),
                diagnostic(Severity::Error, at(0, 2), at(0, 6)),
            ],
            0..1,
        );
        assert_eq!(rows[0].ranges, vec![(2..6, Severity::Error)]);
    }

    #[test]
    fn adjacent_ranges_of_one_severity_join_up() {
        let rows = for_viewport(
            &[
                diagnostic(Severity::Error, at(0, 0), at(0, 3)),
                diagnostic(Severity::Error, at(0, 3), at(0, 6)),
            ],
            0..1,
        );
        assert_eq!(rows[0].ranges, vec![(0..6, Severity::Error)]);
    }

    #[test]
    fn ranges_come_back_in_ascending_order() {
        let rows = for_viewport(
            &[
                diagnostic(Severity::Error, at(0, 20), at(0, 24)),
                diagnostic(Severity::Warning, at(0, 2), at(0, 6)),
            ],
            0..1,
        );
        let starts: Vec<usize> = rows[0].ranges.iter().map(|(r, _)| r.start).collect();
        assert_eq!(starts, vec![2, 20]);
    }

    #[test]
    fn an_empty_viewport_is_not_a_panic() {
        assert!(for_viewport(&[diagnostic(Severity::Error, at(0, 0), at(0, 1))], 0..0).is_empty());
    }
}
