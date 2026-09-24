#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Goto,
    Visual,
    Match,
}

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
