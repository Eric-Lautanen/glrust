use crate::json_schema::Rule;
use glr_core::parse_table::{ParseTable, ParseTableAction, ParseTableEntry};
use glr_core::symbol::{Symbol, SymbolKind};
use glr_core::{StateId, SymbolId};
use std::collections::{BTreeSet, HashMap, HashSet};

type TermSet = BTreeSet<u32>;

type Item = (usize, usize, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    None,
    Left,
    Right,
    NonAssoc,
}

impl Assoc {
    pub fn from_rule(rule: &Rule) -> Self {
        match rule {
            Rule::PrecLeft { .. } => Assoc::Left,
            Rule::PrecRight { .. } => Assoc::Right,
            Rule::Prec { .. } => Assoc::NonAssoc,
            _ => Assoc::None,
        }
    }
}

fn compute_nullable(productions: &[(u32, Vec<u32>, i32)], symbol_count: usize) -> Vec<bool> {
    let mut nullable = vec![false; symbol_count];
    let mut changed = true;
    while changed {
        changed = false;
        for (lhs, rhs, _) in productions {
            let l = *lhs as usize;
            if nullable[l] {
                continue;
            }
            if rhs.iter().all(|&s| nullable[s as usize]) {
                nullable[l] = true;
                changed = true;
            }
        }
    }
    nullable
}

fn compute_first(
    symbols: &[Symbol],
    productions: &[(u32, Vec<u32>, i32)],
    nullable: &[bool],
) -> Vec<TermSet> {
    let count = symbols.len();
    let mut first = vec![TermSet::new(); count];

    for (i, sym) in symbols.iter().enumerate() {
        if sym.kind == SymbolKind::Terminal || sym.kind == SymbolKind::External {
            first[i].insert(u32::try_from(i).expect("symbol count exceeds u32::MAX"));
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (lhs, rhs, _) in productions {
            let l = *lhs as usize;
            let mut added = Vec::new();
            for &sym in rhs {
                let s = sym as usize;
                for &t in &first[s] {
                    if !first[l].contains(&t) {
                        added.push(t);
                    }
                }
                if !nullable[s] {
                    break;
                }
            }
            if !added.is_empty() {
                for t in added {
                    first[l].insert(t);
                }
                changed = true;
            }
        }
    }

    first
}

fn first_of_seq(
    rhs: &[u32],
    start: usize,
    nullable: &[bool],
    first: &[TermSet],
    fallback: u32,
) -> TermSet {
    let mut result = TermSet::new();
    for &sym in rhs.iter().skip(start) {
        let s = sym as usize;
        for &t in &first[s] {
            result.insert(t);
        }
        if !nullable[s] {
            return result;
        }
    }
    result.insert(fallback);
    result
}

fn lr1_closure(
    items: &BTreeSet<Item>,
    all_prods: &[(u32, Vec<u32>, i32)],
    nonterms: &[u32],
    nullable: &[bool],
    first: &[TermSet],
) -> BTreeSet<Item> {
    let mut set = items.clone();
    let mut changed = true;
    while changed {
        changed = false;
        let snapshot: Vec<Item> = set.iter().copied().collect();
        for &(pid, dot, lookahead) in &snapshot {
            let rhs = &all_prods[pid].1;
            if dot < rhs.len() {
                let sym = rhs[dot];
                if nonterms.contains(&sym) {
                    let new_la = first_of_seq(rhs, dot + 1, nullable, first, lookahead);
                    for (qid, (nt, _, _)) in all_prods.iter().enumerate() {
                        if *nt == sym {
                            for &la in &new_la {
                                if set.insert((qid, 0, la)) {
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    set
}

fn lr1_goto(
    items: &BTreeSet<Item>,
    sym: u32,
    all_prods: &[(u32, Vec<u32>, i32)],
    nonterms: &[u32],
    nullable: &[bool],
    first: &[TermSet],
) -> BTreeSet<Item> {
    let mut next: BTreeSet<Item> = BTreeSet::new();
    for &(pid, dot, la) in items {
        let rhs = &all_prods[pid].1;
        if dot < rhs.len() && rhs[dot] == sym {
            next.insert((pid, dot + 1, la));
        }
    }
    lr1_closure(&next, all_prods, nonterms, nullable, first)
}

fn build_lr1_states(
    all_productions: &[(u32, Vec<u32>, i32)],
    nonterminals: &[u32],
    symbol_count: u32,
    nullable: &[bool],
    first: &[TermSet],
) -> (Vec<BTreeSet<Item>>, HashMap<BTreeSet<Item>, usize>) {
    let initial = if all_productions.is_empty() {
        BTreeSet::new()
    } else {
        lr1_closure(
            &BTreeSet::from([(0usize, 0usize, 0u32)]),
            all_productions,
            nonterminals,
            nullable,
            first,
        )
    };

    let mut states: Vec<BTreeSet<Item>> = Vec::new();
    let mut state_map: HashMap<BTreeSet<Item>, usize> = HashMap::new();

    if !initial.is_empty() {
        state_map.insert(initial.clone(), 0);
        states.push(initial);
    }

    let mut i = 0;
    while i < states.len() {
        for sym in 0..symbol_count {
            let next = lr1_goto(
                &states[i],
                sym,
                all_productions,
                nonterminals,
                nullable,
                first,
            );
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

    (states, state_map)
}

fn add_action(
    entries: &mut [ParseTableAction],
    state_idx: usize,
    sym: u32,
    symbol_count: u32,
    action: ParseTableEntry,
) {
    let idx = state_idx * symbol_count as usize + sym as usize;
    if let Some(cell) = entries.get_mut(idx) {
        if cell.is_error() {
            cell.entries.clear();
        }
        if !cell.entries.contains(&action) {
            cell.entries.push(action);
        }
    }
}

struct LrAnalysis<'a> {
    nullable: &'a [bool],
    first: &'a [TermSet],
}

fn fill_parse_entries(
    states: &[BTreeSet<Item>],
    state_map: &HashMap<BTreeSet<Item>, usize>,
    all_productions: &[(u32, Vec<u32>, i32)],
    terminals: &[u32],
    nonterminals: &[u32],
    symbol_count: u32,
    lr: &LrAnalysis<'_>,
) -> Vec<ParseTableAction> {
    let table_size = states.len().saturating_mul(symbol_count as usize);
    let mut entries = vec![ParseTableAction::single(ParseTableEntry::Error); table_size];

    for (si, state) in states.iter().enumerate() {
        for &(pid, dot, lookahead) in state {
            let rhs = &all_productions[pid].1;
            let lhs = all_productions[pid].0;

            if dot < rhs.len() {
                let sym = rhs[dot];
                let target_set = lr1_goto(
                    state,
                    sym,
                    all_productions,
                    nonterminals,
                    lr.nullable,
                    lr.first,
                );
                if let Some(&target_si) = state_map.get(&target_set) {
                    let sid = u32::try_from(target_si).expect("LR state index exceeds u32::MAX");
                    if terminals.contains(&sym) {
                        add_action(
                            &mut entries,
                            si,
                            sym,
                            symbol_count,
                            ParseTableEntry::Shift {
                                state: StateId(sid),
                            },
                        );
                    } else {
                        add_action(
                            &mut entries,
                            si,
                            sym,
                            symbol_count,
                            ParseTableEntry::Goto {
                                state: StateId(sid),
                            },
                        );
                    }
                }
            } else if dot == rhs.len() {
                if pid == 0 {
                    add_action(
                        &mut entries,
                        si,
                        lookahead,
                        symbol_count,
                        ParseTableEntry::Accept,
                    );
                } else {
                    let orig_prod = pid - 1;
                    let child_count =
                        u16::try_from(rhs.len()).expect("RHS length exceeds u16::MAX");
                    let prod_id =
                        u16::try_from(orig_prod).expect("production count exceeds u16::MAX");
                    let reduce_action = ParseTableEntry::Reduce {
                        symbol: SymbolId(lhs),
                        child_count,
                        dynamic_precedence: all_productions[pid].2,
                        production_id: prod_id,
                    };
                    add_action(&mut entries, si, lookahead, symbol_count, reduce_action);
                }
            }
        }
    }

    entries
}

fn build_terminal_precedence(
    symbols: &[Symbol],
    precedence_groups: &[Vec<String>],
) -> HashMap<u32, i32> {
    let mut map = HashMap::new();
    for (level, group) in precedence_groups.iter().enumerate() {
        for name in group {
            for sym in symbols {
                if sym.kind == SymbolKind::Terminal {
                    let sym_name_trim = sym.name.trim_matches('"');
                    let sym_name_inner = if sym.name.starts_with('/') && sym.name.len() > 2 {
                        &sym.name[1..sym.name.len() - 1]
                    } else {
                        sym.name.as_str()
                    };
                    if sym_name_trim == name.as_str() || sym_name_inner == name.as_str() {
                        map.insert(sym.id.0, i32::try_from(level).unwrap_or(i32::MAX));
                    }
                }
            }
        }
    }
    map
}

fn resolve_conflicts(
    entries: &mut [ParseTableAction],
    states: &[BTreeSet<Item>],
    symbol_count: u32,
    symbols: &[Symbol],
    precedence_info: &[(Option<i32>, Assoc)],
    conflict_decls: &[Vec<String>],
    precedence_groups: &[Vec<String>],
) {
    let sc = symbol_count as usize;
    let symbol_names: HashMap<u32, &str> =
        symbols.iter().map(|s| (s.id.0, s.name.as_str())).collect();

    let terminal_prec = build_terminal_precedence(symbols, precedence_groups);
    let conflict_pairs = build_conflict_pairs(conflict_decls);

    for (si, _state) in states.iter().enumerate() {
        for sym in 0..symbol_count {
            let idx = si * sc + sym as usize;
            let cell = &mut entries[idx];
            if cell.is_error() || cell.entries.len() <= 1 {
                continue;
            }

            let shifts: Vec<usize> = cell
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e, ParseTableEntry::Shift { .. }))
                .map(|(i, _)| i)
                .collect();
            let reduces: Vec<usize> = cell
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e, ParseTableEntry::Reduce { .. }))
                .map(|(i, _)| i)
                .collect();

            if shifts.len() == 1 && !reduces.is_empty() {
                if is_declared_conflict(&cell.entries, &conflict_pairs, &symbol_names) {
                    continue;
                }

                let term_prec = terminal_prec.get(&sym).copied();
                let mut resolved_some = false;

                for &ri in &reduces {
                    if let ParseTableEntry::Reduce { production_id, .. } = cell.entries[ri] {
                        let prod_idx = production_id as usize;
                        let prod_info = precedence_info.get(prod_idx).copied();

                        if let (Some(tp), Some((Some(pp), assoc))) = (term_prec, prod_info) {
                            if pp > tp {
                                cell.entries[shifts[0]] = ParseTableEntry::Error;
                                resolved_some = true;
                                break;
                            } else if tp > pp {
                                cell.entries[ri] = ParseTableEntry::Error;
                                resolved_some = true;
                            } else {
                                match assoc {
                                    Assoc::Left => {
                                        cell.entries[shifts[0]] = ParseTableEntry::Error;
                                        resolved_some = true;
                                        break;
                                    }
                                    Assoc::Right => {
                                        cell.entries[ri] = ParseTableEntry::Error;
                                        resolved_some = true;
                                    }
                                    Assoc::NonAssoc => {
                                        cell.entries[shifts[0]] = ParseTableEntry::Error;
                                        cell.entries[ri] = ParseTableEntry::Error;
                                        resolved_some = true;
                                        break;
                                    }
                                    Assoc::None => {}
                                }
                            }
                        }
                    }
                }

                if !resolved_some {
                    let sym_name = symbol_names.get(&sym).unwrap_or(&"<unknown>");
                    let reduce_names: Vec<String> = reduces
                        .iter()
                        .filter_map(|&ri| {
                            if let ParseTableEntry::Reduce {
                                symbol,
                                production_id,
                                ..
                            } = cell.entries[ri]
                            {
                                let name = symbol_names.get(&symbol.0).unwrap_or(&"<unknown>");
                                Some(format!("production {} ({} → ...)", production_id, name))
                            } else {
                                None
                            }
                        })
                        .collect();
                    eprintln!(
                        "Warning: unresolved shift/reduce conflict in state {si} on symbol '{sym_name}'"
                    );
                    eprintln!("  └─ shift on '{sym_name}'");
                    for rn in &reduce_names {
                        eprintln!("  └─ reduce: {rn}");
                    }
                }
            } else if shifts.is_empty() && reduces.len() > 1 {
                if is_declared_conflict(&cell.entries, &conflict_pairs, &symbol_names) {
                    continue;
                }

                let sym_name = symbol_names.get(&sym).unwrap_or(&"<unknown>");
                let reduce_names: Vec<String> = reduces
                    .iter()
                    .filter_map(|&ri| {
                        if let ParseTableEntry::Reduce {
                            symbol,
                            production_id,
                            ..
                        } = cell.entries[ri]
                        {
                            let name = symbol_names.get(&symbol.0).unwrap_or(&"<unknown>");
                            Some(format!("production {} ({} → ...)", production_id, name))
                        } else {
                            None
                        }
                    })
                    .collect();
                eprintln!(
                    "Warning: unresolved reduce/reduce conflict in state {si} on symbol '{sym_name}'"
                );
                for rn in &reduce_names {
                    eprintln!("  └─ reduce: {rn}");
                }
            }
        }
    }

    for cell in entries.iter_mut() {
        cell.entries
            .retain(|e| !matches!(e, ParseTableEntry::Error));
        if cell.entries.is_empty() {
            cell.entries.push(ParseTableEntry::Error);
        }
    }
}

fn build_conflict_pairs(conflict_decls: &[Vec<String>]) -> HashSet<(String, String)> {
    let mut pairs = HashSet::new();
    for group in conflict_decls {
        for i in 0..group.len() {
            for j in i + 1..group.len() {
                pairs.insert((group[i].clone(), group[j].clone()));
                pairs.insert((group[j].clone(), group[i].clone()));
            }
        }
    }
    pairs
}

fn is_declared_conflict(
    entries: &[ParseTableEntry],
    conflict_pairs: &HashSet<(String, String)>,
    symbol_names: &HashMap<u32, &str>,
) -> bool {
    let sym: Vec<&str> = entries
        .iter()
        .filter_map(|e| match e {
            ParseTableEntry::Reduce { symbol, .. } => symbol_names.get(&symbol.0).copied(),
            ParseTableEntry::Shift { .. } => None,
            _ => None,
        })
        .collect();

    for i in 0..sym.len() {
        for j in i + 1..sym.len() {
            if conflict_pairs.contains(&(sym[i].to_string(), sym[j].to_string())) {
                return true;
            }
        }
    }
    false
}

fn split_large_small(
    entries: &[ParseTableAction],
    states: &[BTreeSet<Item>],
    symbol_count: u32,
) -> (
    Vec<ParseTableAction>,
    Vec<glr_core::parse_table::SmallStateRow>,
    u32,
) {
    let count = states.len();
    let sc = symbol_count as usize;
    let large_threshold = 5;

    let mut state_is_small = vec![false; count];
    let mut large_state_count = 0u32;

    for (si, _) in states.iter().enumerate() {
        let mut non_error = 0;
        for sym in 0..sc {
            let idx = si * sc + sym;
            if entries.get(idx).is_none_or(ParseTableAction::is_error) {
                continue;
            }
            non_error += 1;
        }
        state_is_small[si] = non_error > 0 && non_error <= large_threshold;
        if !state_is_small[si] && non_error > 0 {
            large_state_count = u32::try_from(si + 1).expect("large state count exceeds u32::MAX");
        }
    }

    let mut compact_large: Vec<ParseTableAction> = Vec::new();
    for (si, _) in states.iter().enumerate() {
        if !state_is_small[si] {
            for sym in 0..sc {
                let idx = si * sc + sym;
                compact_large.push(entries[idx].clone());
            }
        }
    }

    let mut small_states: Vec<glr_core::parse_table::SmallStateRow> = Vec::new();
    for (si, _) in states.iter().enumerate() {
        if state_is_small[si] {
            let mut row_entries: Vec<(u32, ParseTableAction)> = Vec::new();
            for sym in 0..sc {
                let idx = si * sc + sym;
                let action = &entries[idx];
                if !action.is_error() {
                    row_entries.push((
                        u32::try_from(sym).expect("symbol index exceeds u32::MAX"),
                        action.clone(),
                    ));
                }
            }
            small_states.push(glr_core::parse_table::SmallStateRow::new(row_entries));
        }
    }

    if large_state_count == 0 && !small_states.is_empty() {
        large_state_count = 0;
    }
    if large_state_count == 0 && count > 0 {
        large_state_count = u32::try_from(count).expect("state count exceeds u32::MAX");
    }

    (compact_large, small_states, large_state_count)
}

pub type RawProduction = (
    u32,
    Vec<u32>,
    i32,
    Vec<(u16, u16)>,
    Vec<(u16, u32, bool)>,
    Option<i32>,
    Assoc,
);

pub struct ParseTableGenerator;

impl ParseTableGenerator {
    #[must_use]
    pub fn generate(
        symbols: &[Symbol],
        raw_productions: &[RawProduction],
        start_symbol: u32,
        precedence_groups: &[Vec<String>],
        conflict_decls: &[Vec<String>],
    ) -> ParseTable {
        Self::generate_with_extras(symbols, raw_productions, start_symbol, precedence_groups, conflict_decls, &[])
    }

    #[must_use]
    pub fn generate_with_extras(
        symbols: &[Symbol],
        raw_productions: &[RawProduction],
        start_symbol: u32,
        precedence_groups: &[Vec<String>],
        conflict_decls: &[Vec<String>],
        extra_symbols: &[u32],
    ) -> ParseTable {
        let symbol_count = u32::try_from(symbols.len()).expect("symbol count exceeds u32::MAX");

        let terminals: Vec<u32> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Terminal)
            .map(|s| s.id.0)
            .collect();

        let nonterminals: Vec<u32> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::NonTerminal)
            .map(|s| s.id.0)
            .collect();

        let bare_productions: Vec<(u32, Vec<u32>, i32)> = raw_productions
            .iter()
            .map(|(nt, rhs, prec, _, _, _, _)| (*nt, rhs.clone(), *prec))
            .collect();

        let precedence_info: Vec<(Option<i32>, Assoc)> = raw_productions
            .iter()
            .map(|(_, _, _, _, _, prec_level, assoc)| (*prec_level, *assoc))
            .collect();

        let mut all_productions: Vec<(u32, Vec<u32>, i32)> = Vec::new();
        all_productions.push((start_symbol, vec![start_symbol], 0));
        all_productions.extend(bare_productions);

        let nullable = compute_nullable(&all_productions, symbols.len());
        let first = compute_first(symbols, &all_productions, &nullable);

        let (states, state_map) = build_lr1_states(
            &all_productions,
            &nonterminals,
            symbol_count,
            &nullable,
            &first,
        );

        let lr = LrAnalysis {
            nullable: &nullable,
            first: &first,
        };
        let mut entries = fill_parse_entries(
            &states,
            &state_map,
            &all_productions,
            &terminals,
            &nonterminals,
            symbol_count,
            &lr,
        );

        for &ext in extra_symbols {
            for si in 0..states.len() {
                let entry = ParseTableEntry::Shift {
                    state: StateId(u32::try_from(si).expect("state index exceeds u32::MAX")),
                };
                add_action(&mut entries, si, ext, symbol_count, entry);
            }
        }

        resolve_conflicts(
            &mut entries,
            &states,
            symbol_count,
            symbols,
            &precedence_info,
            conflict_decls,
            precedence_groups,
        );

        let state_count = u32::try_from(states.len()).expect("LR state count exceeds u32::MAX");

        let (compact_large, small_states, large_state_count) =
            split_large_small(&entries, &states, symbol_count);

        ParseTable {
            symbol_count,
            state_count,
            large_state_count,
            large_entries: compact_large,
            small_states,
        }
    }
}
