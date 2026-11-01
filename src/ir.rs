#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourcePos {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Op {
    Move {
        amount: i64,
        pos: SourcePos,
    },
    Add {
        amount: i64,
        pos: SourcePos,
    },
    Clear {
        pos: SourcePos,
    },
    /// Move the current cell into an adjacent cell, adding or subtracting it.
    Transfer {
        offset: i8,
        factor: i8,
        pos: SourcePos,
    },
    Output {
        pos: SourcePos,
    },
    Input {
        pos: SourcePos,
    },
    Loop {
        body: Vec<Op>,
        pos: SourcePos,
    },
}

pub type Program = Vec<Op>;
