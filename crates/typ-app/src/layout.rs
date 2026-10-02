use ratatui::layout::Rect;

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

/// Split the body into `(body, menu)`, the menu `height` rows at the bottom.
///
/// Full width and docked, not floating: the body gives up rows and nothing in
/// it moves sideways (interface §6).
pub fn split_menu(body: Rect, height: u16) -> (Rect, Rect) {
    let height = height.min(body.height);
    let rest = Rect::new(body.x, body.y, body.width, body.height - height);
    let menu = Rect::new(body.x, rest.bottom(), body.width, height);
    (rest, menu)
}

/// A `width` by `height` rect centred in `area`, never larger than it.
///
/// Clamped rather than assumed to fit. A `Rect` wider than the buffer it is
/// drawn into panics on the first write, and the terminal really does get this
/// small: the dock already degrades under 60 columns, so a picker
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

/// Where a hover box goes, given where the cursor is and how much it says.
///
/// **Under the cursor, and the placement is deferred rather than designed.**
/// `visual.md` has an open question about where floating things live and this
/// does not answer it: the box goes below the cursor when there is room and
/// above it when there is not, which is what every editor does and what nobody
/// has to be told.
///
/// Clamped into the frame on both axes, because the alternative is a box that
/// disappears off the right of the screen for anyone editing a long line.
pub fn hover_area(frame: Rect, cursor: (u16, u16), text: &str) -> Rect {
    let lines = text.lines().count();
    let longest = text
        .lines()
        // Cells, not chars: a CJK line counted by chars got a box half as
        // wide as its text. Gap 105.
        .map(typ_buffer::display_width)
        .max()
        .unwrap_or(0);
    // Clamped in `usize` before narrowing: both counts come from a server's
    // text, and `as u16` first wrapped a 65536-character line to a 3-cell box
    // and overflowed the `+ 2` at 65534. Gap 106.
    let fit = |count: usize, max: u16| count.saturating_add(2).clamp(3, max as usize) as u16;
    let width = fit(longest, HOVER_MAX_WIDTH)
        .max(HOVER_MIN_WIDTH)
        .min(frame.width);
    let height = fit(lines, HOVER_MAX_HEIGHT).min(frame.height);

    let (cx, cy) = cursor;
    let x = cx.min(frame.width.saturating_sub(width));
    // Below when it fits, above when it does not. Never over the cursor: the
    // box is about the thing under it. And never over the cursor's row either,
    // counting the float's one-cell gutter (interface §3), so the box sits a
    // row further off than it did, with a row of page in between.
    let y = if cy + 2 + height <= frame.height {
        cy + 2
    } else {
        cy.saturating_sub(height + 1)
    };

    Rect {
        x: frame.x + x,
        y: frame.y + y,
        width,
        height,
    }
}

/// Wide enough for a signature, short enough to leave the code visible.
pub const HOVER_MAX_WIDTH: u16 = 64;

/// What the hover float's border says it is, and what closes it.
pub const HOVER_NAME: &str = "hover";
pub const HOVER_EXIT: &str = "esc";

/// Narrow enough for a one-word answer, wide enough that the border can say
/// both what the box is and how to close it: `╭─ hover ─ esc ─╮`.
pub const HOVER_MIN_WIDTH: u16 = (HOVER_NAME.len() + HOVER_EXIT.len() + 9) as u16;
pub const HOVER_MAX_HEIGHT: u16 = 12;

/// Split a pane's rect into `(tab_row, content)`.
///
/// **One function, because render and hit-testing both need the answer.** The
/// row moves every screen coordinate inside the editor down by one, and two
/// places computing that independently is the drift `picker_area` and
/// `chrome::inner` both exist to prevent.
///
/// Always a row, even for one tab. Before M4 a single tab got no bar, because
/// the panel's box carried the file's name in its border. The box is gone and
/// the tab is the editor's label now (interface §4): without it a lone file
/// would have no name on screen and no place to show it has focus.
pub fn split_tabs(pane: Rect) -> (Rect, Rect) {
    // One row is all there is: give it to the text, the thing being edited.
    if pane.height < 2 {
        return (Rect::new(pane.x, pane.y, pane.width, 0), pane);
    }
    let bar = Rect::new(pane.x, pane.y, pane.width, 1);
    let rest = Rect::new(pane.x, pane.y + 1, pane.width, pane.height - 1);
    (bar, rest)
}
