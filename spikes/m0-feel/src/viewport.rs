use std::ops::Range;

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub top_line: usize,
    pub height: usize,
}

impl Viewport {
    /// Lines currently visible, clamped to the end of the buffer.
    pub fn visible_range(&self, total_lines: usize) -> Range<usize> {
        let start = self.top_line.min(total_lines);
        let end = (start + self.height).min(total_lines);
        start..end
    }

    /// Scroll by `delta` lines. Positive scrolls down.
    ///
    /// The last screenful stays visible rather than scrolling into empty
    /// space, and a buffer shorter than the viewport never scrolls at all.
    pub fn scroll(&mut self, delta: i32, total_lines: usize) {
        let max_top = total_lines.saturating_sub(self.height);
        let next = self.top_line as i64 + delta as i64;
        self.top_line = next.clamp(0, max_top as i64) as usize;
    }
}
