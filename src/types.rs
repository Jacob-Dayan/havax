//! core editing types and state machines across the editor
//!
//! defines cursor coordinates, modal navigation modes, and surround sub-states

/// zero-indexed row and column coordinate within a text buffer
///
/// ordered lexicographically by row then column for straightforward range comparisons
///
/// # Examples
///
/// ```
/// use havax::types::Position;
///
/// let p1 = Position { row: 2, col: 5 };
/// let p2 = Position { row: 3, col: 0 };
/// assert!(p1 < p2);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

/// modal editor input state
///
/// drives keymap dispatching, cursor shapes, and status bar rendering
///
/// # Examples
///
/// ```
/// use havax::types::Mode;
///
/// let mode = Mode::Normal;
/// assert_eq!(mode, Mode::Normal);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Goto,
    Visual,
    Match,
    Leader,
    Replace,
}

/// active sub-state for match mode surround and textobject operations
///
/// tracks multi-key sequences like surround additions, replacements, and inside/around selection
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchState {
    Menu,
    SurroundAdd,
    SurroundReplaceFrom,
    SurroundReplaceTo(char),
    SurroundDelete,
    SelectAround,
    SelectInside,
}
