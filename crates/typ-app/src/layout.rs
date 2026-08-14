use ratatui::layout::Rect;

/// Preferred sidebar width in columns.
const SIDEBAR_WIDTH: u16 = 30;
/// Below this total width the sidebar takes a share instead of a fixed size.
const NARROW_THRESHOLD: u16 = 60;

/// Split the frame into `(tree_area, editor_area)`.
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
