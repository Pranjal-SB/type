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

/// A `width` by `height` rect centred in `area`, never larger than it.
///
/// Clamped rather than assumed to fit. A `Rect` wider than the buffer it is
/// drawn into panics on the first write, and the terminal really does get this
/// small — `split` already degrades the sidebar under 60 columns, so a picker
/// asking for 60 is asking for more than the whole frame on a narrow terminal.
///
/// Integer division puts the remainder at the bottom-right, which is the same
/// bias every other centring in the editor takes.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// Where the picker overlay lands in a frame.
///
/// One function rather than two call sites doing the same arithmetic: `render`
/// draws there and `App::areas` hit-tests there, and a one-cell disagreement
/// between them lands every click a row from the pointer. The gutter learned
/// this at M2.3 through `chrome::inner`.
pub fn picker_area(frame: Rect) -> Rect {
    centered(frame, PICKER_WIDTH, PICKER_HEIGHT)
}

/// Preferred overlay size, clamped to the frame by `centered`.
///
/// Wide enough for a deep path without wrapping, short enough that the file
/// underneath stays visible — the picker is a way to move around the project,
/// and hiding it entirely while you do is disorienting.
pub const PICKER_WIDTH: u16 = 72;
pub const PICKER_HEIGHT: u16 = 18;

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
    // The editor starts on the tree's *last* column, not the one after it.
    //
    // Both panels draw a full box, so without the overlap the tree's right
    // border and the editor's left border land in adjacent cells — two rules
    // touching, in two different colours whenever one panel holds focus, which
    // is the seam. Sharing the column means one vertical on screen, drawn
    // twice, and `chrome::frame` merges the corners into tees. Neither panel
    // has to know what sits beside it; the layout arranges for the collision to
    // be harmless instead.
    let shared = sidebar.saturating_sub(1);
    let editor = Rect::new(
        area.x + shared,
        area.y,
        area.width.saturating_sub(shared),
        area.height,
    );
    (tree, editor)
}
