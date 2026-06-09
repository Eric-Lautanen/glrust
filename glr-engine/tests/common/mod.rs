use glr_core::parse_table::{ParseTable, ParseTableAction, ParseTableEntry};
use glr_core::symbol::{Symbol, SymbolKind};
use glr_core::{Grammar, ProductionId, StateId, SymbolId};
use glr_lexer::{Lexer, Token};
use std::collections::{BTreeSet, HashMap};
use std::vec::Vec;

// ---------------------------------------------------------------------------
// TestGrammarBuilder – builds a correct LR(0) parse table
// ---------------------------------------------------------------------------

pub struct TestGrammarBuilder {
    symbols: Vec<(String, SymbolKind)>,
    productions: Vec<(u32, Vec<u32>, i32)>,
}

impl TestGrammarBuilder {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            productions: Vec::new(),
        }
    }

    pub fn terminal(&mut self, name: &str) -> u32 {
        let id = self.symbols.len() as u32;
        self.symbols.push((name.to_string(), SymbolKind::Terminal));
        id
    }

    pub fn nonterminal(&mut self, name: &str) -> u32 {
        let id = self.symbols.len() as u32;
        self.symbols
            .push((name.to_string(), SymbolKind::NonTerminal));
        id
    }

    pub fn production(&mut self, nt: u32, rhs: Vec<u32>, prec: i32) {
        self.productions.push((nt, rhs, prec));
    }

    pub fn build(&self) -> Grammar {
        let symbol_count = self.symbols.len() as u32;
        let terminal_count = self
            .symbols
            .iter()
            .filter(|(_, k)| *k == SymbolKind::Terminal)
            .count() as u32;

        // Build the Symbol list
        let syms: Vec<Symbol> = self
            .symbols
            .iter()
            .enumerate()
            .map(|(i, (name, kind))| Symbol {
                id: SymbolId(i as u32),
                name: name.clone(),
                kind: *kind,
            })
            .collect();

        // Determine start symbol: the LHS of the first production
        let start_sym = self.productions.first().map(|(nt, _, _)| *nt);

        // Gather terminals and nonterminals
        let terminals: Vec<u32> = self
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, (_, k))| *k == SymbolKind::Terminal)
            .map(|(i, _)| i as u32)
            .collect();
        // Build augmented production: S' → start_symbol
        // Production 0 is the augmented start production.
        let augmented_prod_id: usize = 0;
        let mut all_productions: Vec<(u32, Vec<u32>, i32)> = Vec::new();
        // Augmented: S' → start_sym
        if let Some(start) = start_sym {
            all_productions.push((start, vec![start], 0));
        }
        all_productions.extend(self.productions.clone());

        // LR(0) item = (production_index_in_all, dot_position)
        type Item = (usize, usize);

        // Compute CLOSURE of a set of items
        let closure = |items: &BTreeSet<Item>| -> BTreeSet<Item> {
            let mut set = items.clone();
            let mut changed = true;
            while changed {
                changed = false;
                let snapshot: Vec<Item> = set.iter().copied().collect();
                for &(pid, dot) in &snapshot {
                    let rhs_len = all_productions[pid].1.len();
                    if dot < rhs_len {
                        let sym = all_productions[pid].1[dot];
                        // If sym is a nonterminal, add all its productions at dot 0
                        if self
                            .symbols
                            .get(sym as usize)
                            .map(|(_, k)| *k == SymbolKind::NonTerminal)
                            .unwrap_or(false)
                        {
                            for (qid, (nt, _, _)) in all_productions.iter().enumerate() {
                                if *nt == sym && !set.contains(&(qid, 0)) {
                                    set.insert((qid, 0));
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            set
        };

        // Compute GOTO(state, symbol)
        let goto = |items: &BTreeSet<Item>, sym: u32| -> BTreeSet<Item> {
            let mut next: BTreeSet<Item> = BTreeSet::new();
            for &(pid, dot) in items {
                let rhs_len = all_productions[pid].1.len();
                if dot < rhs_len && all_productions[pid].1[dot] == sym {
                    next.insert((pid, dot + 1));
                }
            }
            closure(&next)
        };

        // Build all LR(0) states
        let initial = start_sym.map_or(BTreeSet::new(), |_start| {
            closure(&BTreeSet::from([(augmented_prod_id, 0)]))
        });

        let mut states: Vec<BTreeSet<Item>> = Vec::new();
        let mut state_map: HashMap<BTreeSet<Item>, usize> = HashMap::new();

        if !initial.is_empty() {
            state_map.insert(initial.clone(), 0);
            states.push(initial);
        }

        let mut i = 0;
        while i < states.len() {
            for sym in 0..symbol_count {
                let next = goto(&states[i], sym);
                if next.is_empty() {
                    continue;
                }
                if !state_map.contains_key(&next) {
                    let idx = states.len();
                    state_map.insert(next.clone(), idx);
                    states.push(next);
                }
            }
            i += 1;
        }

        let state_count = states.len() as u32;

        // Generate parse table entries
        // Each cell can have multiple actions (for GLR conflicts)
        let mut large_entries: Vec<ParseTableAction> = Vec::new();
        large_entries.resize(
            (state_count as usize) * (symbol_count as usize),
            ParseTableAction::single(ParseTableEntry::Error),
        );

        // Helper to add an action to a cell
        let add_action =
            |entries: &mut Vec<ParseTableAction>, s: usize, sym: u32, action: ParseTableEntry| {
                let idx = s * symbol_count as usize + sym as usize;
                let cell = &mut entries[idx];
                if cell.is_error() {
                    cell.entries.clear();
                }
                cell.entries.push(action);
            };

        for (si, state) in states.iter().enumerate() {
            for &(pid, dot) in state {
                let rhs_len = all_productions[pid].1.len();
                let lhs = all_productions[pid].0;

                if dot < rhs_len {
                    // Shift on rhs[dot]
                    let sym = all_productions[pid].1[dot];
                    let target_set = goto(state, sym);
                    if let Some(&target_si) = state_map.get(&target_set) {
                        let is_terminal = self
                            .symbols
                            .get(sym as usize)
                            .map(|(_, k)| *k == SymbolKind::Terminal)
                            .unwrap_or(false);
                        if is_terminal {
                            add_action(
                                &mut large_entries,
                                si,
                                sym,
                                ParseTableEntry::Shift {
                                    state: StateId(target_si as u32),
                                },
                            );
                        } else {
                            add_action(
                                &mut large_entries,
                                si,
                                sym,
                                ParseTableEntry::Goto {
                                    state: StateId(target_si as u32),
                                },
                            );
                        }
                    }
                }

                if dot == rhs_len {
                    // Reduce LR(0): reduce on ALL terminals
                    if pid == augmented_prod_id {
                        // S' → S · — accept
                        for &t in &terminals {
                            add_action(&mut large_entries, si, t, ParseTableEntry::Accept);
                        }
                    } else {
                        let orig_prod = pid - 1;
                        let child_count = rhs_len as u16;
                        let reduce_action = ParseTableEntry::Reduce {
                            symbol: SymbolId(lhs),
                            child_count,
                            dynamic_precedence: 0,
                            production_id: orig_prod as u16,
                        };
                        for &t in &terminals {
                            add_action(&mut large_entries, si, t, reduce_action);
                        }
                    }
                }
            }
        }

        // Build productions list (without the augmented start)
        let productions: Vec<_> = self
            .productions
            .iter()
            .enumerate()
            .map(|(i, (nt, rhs, prec))| glr_core::grammar::Production {
                id: ProductionId(i as u16),
                nonterminal: SymbolId(*nt),
                symbols: rhs.iter().map(|&s| SymbolId(s)).collect(),
                dynamic_precedence: *prec,
            })
            .collect();

        let table = ParseTable {
            symbol_count,
            state_count,
            large_state_count: state_count,
            large_entries,
            small_states: Vec::new(),
        };

        Grammar {
            version: 14,
            symbol_count,
            alias_count: 0,
            token_count: terminal_count,
            external_token_count: 0,
            state_count,
            large_state_count: state_count,
            production_id_count: productions.len() as u32,
            field_count: 0,
            max_alias_sequence_length: 0,
            symbols: syms,
            productions,
            parse_table: table,
        }
    }
}

// ---------------------------------------------------------------------------
// TestLexer – tokenizes based on a static map
// ---------------------------------------------------------------------------

pub struct TestLexer<'a> {
    source: &'a [u8],
    cursor: u32,
    token_map: Vec<(u32, &'a [u8])>,
}

impl<'a> TestLexer<'a> {
    pub fn new(source: &'a [u8], token_map: Vec<(u32, &'a [u8])>) -> Self {
        Self {
            source,
            cursor: 0,
            token_map,
        }
    }
}

impl<'a> Lexer for TestLexer<'a> {
    fn next_token(&mut self, _valid_symbols: &[bool]) -> Option<Token> {
        let len = self.source.len() as u32;
        if self.cursor >= len {
            return None;
        }
        // Skip whitespace
        while self.cursor < len && self.source[self.cursor as usize].is_ascii_whitespace() {
            self.cursor += 1;
        }
        if self.cursor >= len {
            return None;
        }

        let remaining = &self.source[self.cursor as usize..];
        for &(sym_id, literal) in &self.token_map {
            if remaining.starts_with(literal) {
                let start = self.cursor;
                self.cursor += literal.len() as u32;
                return Some(Token {
                    kind: SymbolId(sym_id),
                    start_byte: start,
                    end_byte: self.cursor,
                });
            }
        }

        // Fallback: emit one byte as unknown token
        let start = self.cursor;
        self.cursor += 1;
        Some(Token {
            kind: SymbolId(0),
            start_byte: start,
            end_byte: self.cursor,
        })
    }

    fn cursor(&self) -> u32 {
        self.cursor
    }

    fn reset_to(&mut self, byte_offset: u32) {
        self.cursor = byte_offset;
    }
}
