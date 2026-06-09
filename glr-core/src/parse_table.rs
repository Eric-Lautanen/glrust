use crate::{StateId, SymbolId};

/// Merged parse table: a flat `Vec<Vec<ParseTableEntry>>` indexed by
/// `[state][symbol]`.
#[derive(Debug, Clone)]
pub struct ParseTable {
    pub entries: Vec<Vec<ParseTableEntry>>,
    pub state_count: u32,
    pub symbol_count: u32,
    pub large_state_count: u32,
}

/// An entry in the LR parse table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseTableEntry {
    Shift { state: StateId },
    Reduce { symbol: SymbolId, child_count: u16, dynamic_precedence: i32, production_id: u16 },
    Goto { state: StateId },
    Accept,
    Error,
}
