use crate::{ProductionId, SymbolId};

/// A compiled grammar — the in-memory representation of a language's
/// grammar suitable for the GLR parser engine.
#[derive(Debug, Clone)]
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
    pub productions: Vec<Production>,
}

/// A single grammar production: `Nonterminal → Symbol₁ Symbol₂ … Symbolₙ`.
#[derive(Debug, Clone)]
pub struct Production {
    pub id: ProductionId,
    pub nonterminal: SymbolId,
    pub symbols: Vec<SymbolId>,
    pub dynamic_precedence: i32,
}
