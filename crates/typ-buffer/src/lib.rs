pub mod buffer;
pub mod position;
pub mod search;
pub mod selection;
pub mod undo;
pub mod word;

pub use buffer::TextBuffer;
pub use position::{
    Position, display_to_grapheme_col, display_width, display_width_with_tabs,
    grapheme_to_display_col,
};
pub use search::{SearchQuery, find_in_line};
pub use selection::{Selection, Selections};
pub use undo::EditKind;
pub use word::{next_word_boundary, previous_word_boundary, word_at};
