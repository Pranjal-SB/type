//! A session: what a project looked like when the editor quit on it, so the
//! next start on that project looks the same.
//!
//! Data and files only. The app turns itself into a [`Session`] and back
//! (`app/sessions.rs`); this says how one is written down and where.
//!
//! Stored as TOML in the state directory, not the config directory: a session
//! is something the editor wrote, not something the user did, and a dotfiles
//! repo that tracks the config should not churn on every quit.

use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::dock::Preset;
use crate::pane::{Axis, Layout, PaneId};

/// One file in a pane, and where its view was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTab {
    pub path: PathBuf,
    /// The primary cursor. `col` is a grapheme index, invariant 4.
    pub line: usize,
    pub col: usize,
    /// The first line on screen.
    pub top: usize,
}

/// One pane: its files and which one is on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedPane {
    pub id: PaneId,
    pub tabs: Vec<SavedTab>,
    pub active: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub layout: Layout,
    pub panes: Vec<SavedPane>,
    /// The pane that had the keys.
    pub current: PaneId,
    pub preset: Preset,
    /// The layout was picked by hand, so adapting to width leaves it alone.
    pub picked: bool,
    pub dock_open: bool,
    pub dock_size: u16,
}

/// Where state lives: `TYP_STATE_DIR`, else the platform's state directory.
pub fn state_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TYP_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var("LOCALAPPDATA").ok()?)
    } else if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(xdg)
    } else {
        PathBuf::from(std::env::var("HOME").ok()?).join(".local/state")
    };
    Some(base.join("typ"))
}

/// The session file for the project at `root`, named from its canonical path
/// with everything but letters, digits, `-` and `.` turned into `_`.
///
/// ponytail: `a-b` and `a_b` beside each other share a file, and a path over
/// the filesystem's name limit fails to save (with a warning); a hash of the
/// path fixes both if either is ever met.
pub fn file_for(root: &Path) -> Option<PathBuf> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let name: String = root
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some(state_dir()?.join("sessions").join(format!("{name}.toml")))
}

/// Read the session at `path`. `Ok(None)` when there is none, the normal case.
pub fn load(path: &Path) -> Result<Option<Session>, String> {
    match crate::config::read_config(path)? {
        None => Ok(None),
        Some(text) => Session::from_toml(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
    }
}

/// Write `session` to `path`, through a temp file and a rename, so a crash
/// mid-write leaves the old session rather than half of a new one.
pub fn save(path: &Path, session: &Session) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, session.to_toml())?;
    std::fs::rename(&temp, path)
}

impl Session {
    pub fn to_toml(&self) -> String {
        let mut table = Table::new();
        table.insert("preset".into(), self.preset.name().into());
        table.insert("picked".into(), self.picked.into());
        table.insert("dock_open".into(), self.dock_open.into());
        table.insert("dock_size".into(), i64::from(self.dock_size).into());
        table.insert("current".into(), i64::from(self.current.0).into());
        table.insert("layout".into(), layout_to_value(&self.layout));
        let panes = self.panes.iter().map(pane_to_value).collect::<Vec<_>>();
        table.insert("pane".into(), Value::Array(panes));
        table.to_string()
    }

    /// Parse a session, refusing anything that does not describe a layout the
    /// app could show: every pane in the tree once, and nothing else.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let table: Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let preset = Preset::from_name(str_of(&table, "preset")?).ok_or("unknown preset")?;
        let layout = layout_from_value(table.get("layout").ok_or("no layout")?)?;
        let panes = table
            .get("pane")
            .and_then(Value::as_array)
            .ok_or("no panes")?
            .iter()
            .map(pane_from_value)
            .collect::<Result<Vec<_>, _>>()?;
        let mut in_tree = layout.ids();
        let mut listed: Vec<PaneId> = panes.iter().map(|p| p.id).collect();
        in_tree.sort();
        listed.sort();
        if in_tree != listed {
            return Err("the panes do not match the layout".into());
        }
        let current = PaneId(int_of(&table, "current")? as u32);
        if !listed.contains(&current) {
            return Err("the current pane is not in the layout".into());
        }
        Ok(Session {
            layout,
            panes,
            current,
            preset,
            picked: bool_of(&table, "picked")?,
            dock_open: bool_of(&table, "dock_open")?,
            dock_size: int_of(&table, "dock_size")?.min(u16::MAX as usize) as u16,
        })
    }
}

fn layout_to_value(layout: &Layout) -> Value {
    let mut table = Table::new();
    match layout {
        Layout::Leaf(id) => {
            table.insert("pane".into(), i64::from(id.0).into());
        }
        Layout::Split { axis, ratio, a, b } => {
            let axis = match axis {
                Axis::Columns => "columns",
                Axis::Rows => "rows",
            };
            table.insert("axis".into(), axis.into());
            table.insert("ratio".into(), f64::from(*ratio).into());
            table.insert("a".into(), layout_to_value(a));
            table.insert("b".into(), layout_to_value(b));
        }
    }
    Value::Table(table)
}

fn layout_from_value(value: &Value) -> Result<Layout, String> {
    let table = value.as_table().ok_or("a layout is not a table")?;
    if table.contains_key("pane") {
        return Ok(Layout::Leaf(PaneId(int_of(table, "pane")? as u32)));
    }
    let axis = match str_of(table, "axis")? {
        "columns" => Axis::Columns,
        "rows" => Axis::Rows,
        other => return Err(format!("axis {other} is not columns or rows")),
    };
    let ratio = table
        .get("ratio")
        .and_then(Value::as_float)
        .filter(|r| (0.0..=1.0).contains(r))
        .ok_or("a split's ratio is not between 0 and 1")? as f32;
    let a = layout_from_value(table.get("a").ok_or("a split has no a")?)?;
    let b = layout_from_value(table.get("b").ok_or("a split has no b")?)?;
    Ok(Layout::Split {
        axis,
        ratio,
        a: Box::new(a),
        b: Box::new(b),
    })
}

fn pane_to_value(pane: &SavedPane) -> Value {
    let mut table = Table::new();
    table.insert("id".into(), i64::from(pane.id.0).into());
    table.insert("active".into(), (pane.active as i64).into());
    let tabs = pane
        .tabs
        .iter()
        .map(|tab| {
            let mut t = Table::new();
            t.insert(
                "path".into(),
                tab.path.to_string_lossy().into_owned().into(),
            );
            t.insert("line".into(), (tab.line as i64).into());
            t.insert("col".into(), (tab.col as i64).into());
            t.insert("top".into(), (tab.top as i64).into());
            Value::Table(t)
        })
        .collect();
    table.insert("tab".into(), Value::Array(tabs));
    Value::Table(table)
}

fn pane_from_value(value: &Value) -> Result<SavedPane, String> {
    let table = value.as_table().ok_or("a pane is not a table")?;
    let tabs = match table.get("tab") {
        None => Vec::new(),
        Some(tabs) => tabs
            .as_array()
            .ok_or("a pane's tabs are not a list")?
            .iter()
            .map(|tab| {
                let t = tab.as_table().ok_or("a tab is not a table")?;
                Ok(SavedTab {
                    path: PathBuf::from(str_of(t, "path")?),
                    line: int_of(t, "line")?,
                    col: int_of(t, "col")?,
                    top: int_of(t, "top")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    Ok(SavedPane {
        id: PaneId(int_of(table, "id")? as u32),
        active: int_of(table, "active")?,
        tabs,
    })
}

fn str_of<'a>(table: &'a Table, key: &str) -> Result<&'a str, String> {
    table
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is missing or not text"))
}

fn int_of(table: &Table, key: &str) -> Result<usize, String> {
    table
        .get(key)
        .and_then(Value::as_integer)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n <= u32::MAX as usize)
        .ok_or_else(|| format!("{key} is missing or not a count"))
}

fn bool_of(table: &Table, key: &str) -> Result<bool, String> {
    table
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{key} is missing or not true or false"))
}
