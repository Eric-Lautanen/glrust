use crate::SymbolId;

/// A grammar symbol — either a terminal (token) or nonterminal (rule LHS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
}

/// Distinguishes terminals from nonterminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Terminal,
    NonTerminal,
    External,
}
