pub mod action;
pub mod audit;
pub mod chrome;
pub mod colour;
pub mod diagnostic;
pub mod event;
pub mod key;
pub mod keymap;
pub mod panel;
pub mod style;
pub mod theme;

pub use action::{Action, Direction, Group, Motion};
pub use audit::audit;
pub use chrome::printable;
pub use colour::{Depth, downgrade};
pub use diagnostic::{Diagnostic, Severity};
pub use event::{AppEvent, HandlerId, NotifyLevel, PanelEvent, PanelId};
pub use key::KeyChord;
pub use keymap::{Keymap, Resolved};
pub use panel::{Panel, RenderContext, ThemeColors};
pub use style::{UNDERCURL, Undercurl};
pub use theme::{Kind, SyntaxTheme, Theme};
/// Re-exported because `PanelEvent::OpenFile` carries one, and a panel crate
/// should not need `typ-buffer` in its manifest to say where to open a file.
pub use typ_buffer::Position;
