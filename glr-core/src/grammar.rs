use crate::parse_table::ParseTable;
use crate::symbol::Symbol;
use crate::{ProductionId, SymbolId};
use alloc::vec::Vec;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Grammar {
    pub version: u32,
    pub symbol_count: u32,
    pub alias_count: u32,
    pub token_count: u32,
    pub external_token_count: u32,
    pub state_count: u32,
    pub large_state_count: u32,
    pub production_id_count: u32,
    pub field_count: u32,
    pub max_alias_sequence_length: u32,
    pub symbols: Vec<Symbol>,
    pub productions: Vec<Production>,
    pub parse_table: ParseTable,
}

impl Grammar {
    pub fn symbol_name(&self, id: SymbolId) -> &str {
        self.symbols
            .get(id.0 as usize)
            .map(|s| s.name.as_str())
            .unwrap_or("<unknown>")
    }

    pub fn production(&self, id: ProductionId) -> Option<&Production> {
        self.productions.get(id.0 as usize)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Production {
    pub id: ProductionId,
    pub nonterminal: SymbolId,
    pub symbols: Vec<SymbolId>,
    pub dynamic_precedence: i32,
}
