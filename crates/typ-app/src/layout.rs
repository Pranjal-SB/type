use ratatui::layout::Rect;

/// Preferred sidebar width in columns.
const SIDEBAR_WIDTH: u16 = 30;
/// Below this total width the sidebar takes a share instead of a fixed size.
const NARROW_THRESHOLD: u16 = 60;

/// Split the frame into `(body, status_bar)`.
///
/// The status bar is one row and is never optional: it is where the editor asks
/// questions — unsaved changes, errors — and a prompt with nowhere to appear is
/// a prompt that gets skipped.
pub fn split_frame(area: Rect) -> (Rect, Rect) {
    let body_height = area.height.saturating_sub(1);
    let body = Rect::new(area.x, area.y, area.width, body_height);
    let status = Rect::new(area.x, area.y + body_height, area.width, 1);
    (body, status)
}

/// Split the body into `(tree_area, editor_area)`.
///
/// A fixed sidebar matches what people arriving from GUI editors expect. On
/// narrow terminals a fixed 30 columns would leave nothing for the editor, so
/// it degrades to a third of the width.
pub fn split(area: Rect) -> (Rect, Rect) {
    let sidebar = if area.width < NARROW_THRESHOLD {
        (area.width / 3).max(1)
    } else {
        SIDEBAR_WIDTH
    };
    let tree = Rect::new(area.x, area.y, sidebar, area.height);
    let editor = Rect::new(
        area.x + sidebar,
        area.y,
        area.width.saturating_sub(sidebar),
        area.height,
    );
    (tree, editor)
}
