use crate::json_schema::{GrammarJson, PrecedenceValue, Rule};
use crate::table_gen::ParseTableGenerator;
use glr_core::parse_table::ParseTable;
use glr_core::symbol::{Symbol, SymbolKind};
use glr_core::DfaTable;
use glr_core::{Grammar, Production, ProductionId, SymbolId};
use glr_lexer::DfaTableExt;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub enum CompileError {
    JsonParse(String),
    EmptyGrammar,
    InvalidInline(String),
    Unsupported(String),
    CyclicInline(String),
    DuplicateDefinition(String),
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CompileError::JsonParse(e) => write!(f, "JSON parse error: {e}"),
            CompileError::EmptyGrammar => write!(f, "grammar has no rules"),
            CompileError::InvalidInline(e) => write!(f, "invalid inline rule: {e}"),
            CompileError::Unsupported(e) => write!(f, "unsupported construct: {e}"),
            CompileError::CyclicInline(e) => write!(f, "cyclic inline rule: {e}"),
            CompileError::DuplicateDefinition(e) => write!(f, "duplicate definition: {e}"),
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Debug, Clone)]
struct CompiledRhs {
    symbols: Vec<u32>,
    fields: Vec<(u16, u16)>,
    aliases: Vec<(u16, u32, bool)>,
}

struct FieldTable {
    name_to_id: HashMap<String, u16>,
    names: Vec<String>,
}

impl FieldTable {
    fn new() -> Self {
        Self {
            name_to_id: HashMap::new(),
            names: Vec::new(),
        }
    }

    fn register(&mut self, name: &str) -> u16 {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }
        let id = u16::try_from(self.names.len()).expect("field count exceeds u16::MAX");
        self.name_to_id.insert(name.to_string(), id);
        self.names.push(name.to_string());
        id
    }
}

fn extract_precedence(rule: &Rule) -> i32 {
    match rule {
        Rule::Prec { value, .. } => match value {
            PrecedenceValue::Number(n) => *n,
            PrecedenceValue::Named(_) => 0,
        },
        Rule::PrecDynamic { value, .. } => *value,
        _ => 0,
    }
}

struct CompileCtx {
    name_to_id: HashMap<String, u32>,
    names: Vec<String>,
    kinds: Vec<SymbolKind>,
    next_id: u32,
    repeat_counter: u32,
    productions: Vec<ProdEntry>,
    field_table: FieldTable,
}

type ProdEntry = (
    u32,                     // nonterminal
    Vec<u32>,                // rhs symbols
    i32,                     // dynamic_precedence
    Vec<(u16, u16)>,         // field_map: (child_index, field_id)
    Vec<(u16, u32, bool)>,   // alias_map: (child_index, alias_symbol_id, is_named)
    Option<i32>,             // precedence_level for conflict resolution
    crate::table_gen::Assoc, // associativity for conflict resolution
);

impl CompileCtx {
    fn new() -> Self {
        Self {
            name_to_id: HashMap::new(),
            names: Vec::new(),
            kinds: Vec::new(),
            next_id: 0,
            repeat_counter: 0,
            productions: Vec::new(),
            field_table: FieldTable::new(),
        }
    }

    fn register_nonterminal(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.name_to_id.insert(name.to_string(), id);
        self.names.push(name.to_string());
        self.kinds.push(SymbolKind::NonTerminal);
        id
    }

    fn get_or_create_string(&mut self, value: &str) -> u32 {
        let key = format!("\"{value}\"");
        if let Some(&id) = self.name_to_id.get(&key) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.name_to_id.insert(key.clone(), id);
        self.names.push(key);
        self.kinds.push(SymbolKind::Terminal);
        id
    }

    fn get_or_create_pattern(&mut self, value: &str) -> u32 {
        let key = format!("/{value}/");
        if let Some(&id) = self.name_to_id.get(&key) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.name_to_id.insert(key.clone(), id);
        self.names.push(key);
        self.kinds.push(SymbolKind::Terminal);
        id
    }

    fn get_or_create_external(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.name_to_id.insert(name.to_string(), id);
        self.names.push(name.to_string());
        self.kinds.push(SymbolKind::External);
        id
    }

    fn lookup(&self, name: &str) -> Option<u32> {
        self.name_to_id.get(name).copied()
    }

    fn add_repeat_nonterminal(&mut self, content: &Rule) -> u32 {
        let name = format!("_repeat_{}", self.repeat_counter);
        self.repeat_counter += 1;
        let id = self.register_nonterminal(&name);

        let content_rhs = self.compile_single(&flatten_choice(content));

        let mut rhs = content_rhs.symbols.clone();
        rhs.push(id);
        self.productions.push((
            id,
            rhs,
            0,
            Vec::new(),
            Vec::new(),
            None,
            crate::table_gen::Assoc::None,
        ));

        self.productions.push((
            id,
            vec![],
            0,
            Vec::new(),
            Vec::new(),
            None,
            crate::table_gen::Assoc::None,
        ));

        id
    }

    fn add_repeat1_nonterminal(&mut self, content: &Rule) -> u32 {
        let name = format!("_repeat1_{}", self.repeat_counter);
        self.repeat_counter += 1;
        let id = self.register_nonterminal(&name);

        let content_rhs = self.compile_single(&flatten_choice(content));

        self.productions.push((
            id,
            content_rhs.symbols.clone(),
            0,
            Vec::new(),
            Vec::new(),
            None,
            crate::table_gen::Assoc::None,
        ));

        let mut rhs = content_rhs.symbols;
        rhs.push(id);
        self.productions.push((
            id,
            rhs,
            0,
            Vec::new(),
            Vec::new(),
            None,
            crate::table_gen::Assoc::None,
        ));

        id
    }

    fn compile_single(&mut self, rules: &[&Rule]) -> CompiledRhs {
        let mut symbols = Vec::new();
        let mut fields = Vec::new();
        let mut aliases = Vec::new();

        for rule in rules {
            let start_idx = symbols.len();
            match rule {
                Rule::Seq { members } => {
                    let sub_rules: Vec<&Rule> = members.iter().collect();
                    let sub = self.compile_single(&sub_rules);
                    symbols.extend(sub.symbols);
                    fields.extend(sub.fields.into_iter().map(|(ci, fi)| {
                        (
                            u16::try_from(start_idx + ci as usize)
                                .expect("field child index exceeds u16::MAX"),
                            fi,
                        )
                    }));
                    aliases.extend(sub.aliases.into_iter().map(|(ci, s, n)| {
                        (
                            u16::try_from(start_idx + ci as usize)
                                .expect("alias child index exceeds u16::MAX"),
                            s,
                            n,
                        )
                    }));
                }
                Rule::Symbol { name } => {
                    let id = self
                        .lookup(name)
                        .unwrap_or_else(|| self.get_or_create_string(name));
                    symbols.push(id);
                }
                Rule::String { value } => {
                    let id = self.get_or_create_string(value);
                    symbols.push(id);
                }
                Rule::Pattern { value, .. } => {
                    let id = self.get_or_create_pattern(value);
                    symbols.push(id);
                }
                Rule::Blank | Rule::Choice { .. } => {}
                Rule::Repeat { content } => {
                    let nt_id = self.add_repeat_nonterminal(content);
                    symbols.push(nt_id);
                }
                Rule::Repeat1 { content } => {
                    let nt_id = self.add_repeat1_nonterminal(content);
                    symbols.push(nt_id);
                }
                Rule::Field { name, content } => {
                    let fid = self.field_table.register(name);
                    let sub_rules = &[content.as_ref()];
                    let sub = self.compile_single(sub_rules);
                    for (i, &sym_id) in sub.symbols.iter().enumerate() {
                        symbols.push(sym_id);
                        let ci = u16::try_from(start_idx + i)
                            .expect("field child index exceeds u16::MAX");
                        fields.push((ci, fid));
                    }
                    for (ci, s, n) in sub.aliases {
                        aliases.push((
                            u16::try_from(start_idx + ci as usize)
                                .expect("alias child index exceeds u16::MAX"),
                            s,
                            n,
                        ));
                    }
                }
                Rule::Alias {
                    content,
                    named,
                    value,
                } => {
                    let alias_id = self.get_or_create_string(value);
                    let sub_rules = &[content.as_ref()];
                    let sub = self.compile_single(sub_rules);
                    for (i, &sym_id) in sub.symbols.iter().enumerate() {
                        symbols.push(sym_id);
                        let ci = u16::try_from(start_idx + i)
                            .expect("alias child index exceeds u16::MAX");
                        aliases.push((ci, alias_id, *named));
                    }
                    for (ci, s, n) in sub.aliases {
                        aliases.push((
                            u16::try_from(start_idx + ci as usize)
                                .expect("alias child index exceeds u16::MAX"),
                            s,
                            n,
                        ));
                    }
                }
                Rule::Token { content }
                | Rule::ImmediateToken { content }
                | Rule::Prec { content, .. }
                | Rule::PrecLeft { content, .. }
                | Rule::PrecRight { content, .. }
                | Rule::PrecDynamic { content, .. }
                | Rule::Reserved { content, .. } => {
                    let sub = self.compile_single(&[content.as_ref()]);
                    symbols.extend(sub.symbols);
                    fields.extend(sub.fields.into_iter().map(|(ci, fi)| {
                        (
                            u16::try_from(start_idx + ci as usize)
                                .expect("field child index exceeds u16::MAX"),
                            fi,
                        )
                    }));
                    aliases.extend(sub.aliases.into_iter().map(|(ci, s, n)| {
                        (
                            u16::try_from(start_idx + ci as usize)
                                .expect("alias child index exceeds u16::MAX"),
                            s,
                            n,
                        )
                    }));
                }
                Rule::Unknown(v) => {
                    let _ = v;
                }
            }
        }

        CompiledRhs {
            symbols,
            fields,
            aliases,
        }
    }
}

fn flatten_choice(rule: &Rule) -> Vec<&Rule> {
    match rule {
        Rule::Choice { members } => members.iter().collect(),
        other => vec![other],
    }
}

fn build_empty_grammar() -> Grammar {
    Grammar {
        format_version: 1,
        version: 15,
        min_compatible_version: 13,
        symbol_count: 0,
        alias_count: 0,
        token_count: 0,
        external_token_count: 0,
        state_count: 0,
        large_state_count: 0,
        production_id_count: 0,
        field_count: 0,
        max_alias_sequence_length: 0,
        symbols: Vec::new(),
        productions: Vec::new(),
        parse_table: glr_core::parse_table::ParseTable {
            symbol_count: 0,
            state_count: 0,
            large_state_count: 0,
            large_entries: Vec::new(),
            small_states: Vec::new(),
        },
        fields: Vec::new(),
        supertypes: Vec::new(),
        word_token: None,
        word_symbol_id: None,
        precedence_groups: Vec::new(),
        conflict_decls: Vec::new(),
        reserved: Vec::new(),
        dfa_table: DfaTable::new(Vec::new()),
        line_start_offsets: Vec::new(),
    }
}

fn expand_inline(
    rule: &Rule,
    rules: &HashMap<String, Rule>,
    inline_set: &HashSet<String>,
    depth: usize,
) -> Result<Rule, CompileError> {
    if depth > 100 {
        return Err(CompileError::CyclicInline(
            "inline expansion exceeded 100 levels".to_string(),
        ));
    }
    match rule {
        Rule::Symbol { name } if inline_set.contains(name) => {
            if let Some(body) = rules.get(name) {
                expand_inline(body, rules, inline_set, depth + 1)
            } else {
                Err(CompileError::InvalidInline(format!(
                    "inline rule '{name}' not found"
                )))
            }
        }
        Rule::Seq { members } => {
            let mut new_members = Vec::with_capacity(members.len());
            for m in members {
                let expanded = expand_inline(m, rules, inline_set, depth)?;
                if let Rule::Seq { members: inner } = expanded {
                    new_members.extend(inner);
                } else {
                    new_members.push(expanded);
                }
            }
            Ok(if new_members.len() == 1 {
                new_members.into_iter().next().unwrap()
            } else {
                Rule::Seq {
                    members: new_members,
                }
            })
        }
        Rule::Choice { members } => {
            let new_members: Result<Vec<Rule>, _> = members
                .iter()
                .map(|m| expand_inline(m, rules, inline_set, depth))
                .collect();
            Ok(Rule::Choice {
                members: new_members?,
            })
        }
        Rule::Field { name, content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Field {
                name: name.clone(),
                content: Box::new(new_content),
            })
        }
        Rule::Alias {
            content,
            named,
            value,
        } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Alias {
                content: Box::new(new_content),
                named: *named,
                value: value.clone(),
            })
        }
        Rule::Token { content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Token {
                content: Box::new(new_content),
            })
        }
        Rule::ImmediateToken { content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::ImmediateToken {
                content: Box::new(new_content),
            })
        }
        Rule::Prec { value, content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Prec {
                value: value.clone(),
                content: Box::new(new_content),
            })
        }
        Rule::PrecLeft { value, content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::PrecLeft {
                value: value.clone(),
                content: Box::new(new_content),
            })
        }
        Rule::PrecRight { value, content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::PrecRight {
                value: value.clone(),
                content: Box::new(new_content),
            })
        }
        Rule::PrecDynamic { value, content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::PrecDynamic {
                value: *value,
                content: Box::new(new_content),
            })
        }
        Rule::Repeat { content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Repeat {
                content: Box::new(new_content),
            })
        }
        Rule::Repeat1 { content } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Repeat1 {
                content: Box::new(new_content),
            })
        }
        Rule::Reserved {
            context_name,
            content,
        } => {
            let new_content = expand_inline(content, rules, inline_set, depth)?;
            Ok(Rule::Reserved {
                context_name: context_name.clone(),
                content: Box::new(new_content),
            })
        }
        other => Ok(other.clone()),
    }
}

fn compile_rules(
    ctx: &mut CompileCtx,
    start_id: u32,
    extra_symbols: &[u32],
    precedence_groups: &[Vec<String>],
    conflict_decls: &[Vec<String>],
) -> (Vec<Symbol>, u32, u32, ParseTable, Vec<Production>, DfaTable) {
    let next_id = ctx.next_id;
    let final_symbols: Vec<Symbol> = (0..next_id)
        .map(|i| Symbol {
            id: SymbolId(i),
            name: ctx.names[i as usize].clone(),
            kind: ctx.kinds[i as usize],
        })
        .collect();

    let token_count = u32::try_from(
        final_symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Terminal)
            .count(),
    )
    .unwrap_or(0);

    let external_token_count = u32::try_from(
        final_symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::External)
            .count(),
    )
    .unwrap_or(0);

    let parse_table = ParseTableGenerator::generate_with_extras(
        &final_symbols,
        &ctx.productions,
        start_id,
        precedence_groups,
        conflict_decls,
        extra_symbols,
    );

    let productions: Vec<Production> = ctx
        .productions
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let (nt, rhs, prec, fields, aliases, _, _) = entry;
            let pid = u16::try_from(i).expect("production count exceeds u16::MAX");
            let alias_map: Vec<(u16, SymbolId, bool)> = aliases
                .iter()
                .map(|(ci, s, n)| (*ci, SymbolId(*s), *n))
                .collect();
            Production {
                id: ProductionId(pid),
                nonterminal: SymbolId(*nt),
                symbols: rhs.iter().map(|&s| SymbolId(s)).collect(),
                dynamic_precedence: *prec,
                field_map: fields.clone(),
                alias_map,
            }
        })
        .collect();

    let dfa_table = build_dfa_table(&final_symbols);

    (
        final_symbols,
        token_count,
        external_token_count,
        parse_table,
        productions,
        dfa_table,
    )
}

pub fn compile_grammar(input: &str) -> Result<Grammar, CompileError> {
    let grammar_json: GrammarJson =
        serde_json::from_str(input).map_err(|e| CompileError::JsonParse(e.to_string()))?;

    let mut rule_names: Vec<&String> = grammar_json.rules.keys().collect();
    rule_names.sort_unstable();
    let start_name: String = rule_names
        .first()
        .map(|s| (*s).clone())
        .unwrap_or_default();

    if start_name.is_empty() || grammar_json.rules.is_empty() {
        return Ok(build_empty_grammar());
    }

    let inline_set: HashSet<String> = grammar_json.inline.iter().cloned().collect();

    let mut ctx = CompileCtx::new();

    let inlined_rules: HashMap<String, Rule> = if inline_set.is_empty() {
        grammar_json.rules.clone()
    } else {
        let mut result = HashMap::new();
        for (name, rule) in &grammar_json.rules {
            let expanded = expand_inline(rule, &grammar_json.rules, &inline_set, 0)?;
            result.insert(name.clone(), expanded);
        }
        result
    };

    for rule_name in grammar_json.rules.keys() {
        if inline_set.contains(rule_name) {
            continue;
        }
        ctx.register_nonterminal(rule_name);
    }

    for ext in &grammar_json.externals {
        if let Rule::Symbol { name } = ext {
            ctx.get_or_create_external(name);
        }
    }

    let start_id = ctx.lookup(&start_name).unwrap_or(0);

    for rule_value in inlined_rules.values() {
        discover_symbols(rule_value, &mut ctx, &grammar_json.externals);
    }
    for extra in &grammar_json.extras {
        discover_symbols(extra, &mut ctx, &grammar_json.externals);
    }

    let extra_symbol_ids: Vec<u32> = grammar_json
        .extras
        .iter()
        .filter_map(|extra| {
            let name = match extra {
                Rule::String { value } => format!("\"{value}\""),
                Rule::Pattern { value, .. } => format!("/{value}/"),
                Rule::Symbol { name } => name.clone(),
                _ => return None,
            };
            ctx.lookup(&name)
        })
        .collect();

    let precedence_groups_strs = grammar_json.precedences.clone();

    let precedence_names: HashMap<String, usize> = grammar_json
        .precedences
        .iter()
        .enumerate()
        .flat_map(|(level, group)| group.iter().map(move |name| (name.clone(), level)))
        .collect();

    fn resolve_prec_value(
        value: &PrecedenceValue,
        precedence_names: &HashMap<String, usize>,
    ) -> Option<i32> {
        match value {
            PrecedenceValue::Number(n) => Some(*n),
            PrecedenceValue::Named(name) => precedence_names.get(name).copied().map(|i| i as i32),
        }
    }

    for rule_name in grammar_json.rules.keys() {
        if inline_set.contains(rule_name) {
            continue;
        }
        let nt_id = ctx.lookup(rule_name).unwrap_or(0);
        let rule_value = &inlined_rules[rule_name];
        let alternatives = flatten_choice(rule_value);
        for alt in alternatives {
            let alt_members: Vec<&Rule> = match alt {
                Rule::Seq { members } => members.iter().collect(),
                other => vec![other],
            };
            let prec = extract_precedence(alt);
            let prec_level = match alt {
                Rule::Prec { value, .. } | Rule::PrecLeft { value, .. } | Rule::PrecRight { value, .. } => {
                    resolve_prec_value(value, &precedence_names)
                }
                _ => None,
            };
            let assoc = crate::table_gen::Assoc::from_rule(alt);
            let rhs = ctx.compile_single(&alt_members);
            ctx.productions.push((
                nt_id,
                rhs.symbols,
                prec,
                rhs.fields,
                rhs.aliases,
                prec_level,
                assoc,
            ));
        }
    }

    let (final_symbols, token_count, external_token_count, parse_table, productions, dfa_table) =
        compile_rules(
            &mut ctx,
            start_id,
            &extra_symbol_ids,
            &precedence_groups_strs,
            &grammar_json.conflicts,
        );

    let production_count = u32::try_from(productions.len()).unwrap_or(0);

    let alias_count =
        u32::try_from(productions.iter().map(|p| p.alias_map.len()).sum::<usize>()).unwrap_or(0);
    let max_alias_sequence_length = u32::try_from(
        productions
            .iter()
            .map(|p| p.alias_map.len())
            .max()
            .unwrap_or(0),
    )
    .unwrap_or(0);

    let word_symbol_id = grammar_json.word.as_ref().and_then(|name| {
        final_symbols
            .iter()
            .position(|s| s.name == *name)
            .map(|i| SymbolId(u32::try_from(i).unwrap()))
    });

    let reserved: Vec<(String, Vec<String>)> = grammar_json
        .reserved
        .iter()
        .map(|(ctx_name, rules)| {
            let names: Vec<String> = rules
                .iter()
                .filter_map(|r| match r {
                    Rule::String { value } => Some(value.clone()),
                    Rule::Pattern { value, .. } => Some(format!("/{value}/")),
                    _ => None,
                })
                .collect();
            (ctx_name.clone(), names)
        })
        .collect();

    let grammar = Grammar {
        format_version: 1,
        version: 15,
        min_compatible_version: 13,
        symbol_count: ctx.next_id,
        alias_count,
        token_count,
        external_token_count,
        state_count: parse_table.state_count,
        large_state_count: parse_table.large_state_count,
        production_id_count: production_count,
        field_count: u32::try_from(ctx.field_table.names.len()).unwrap_or(0),
        max_alias_sequence_length,
        symbols: final_symbols,
        productions,
        parse_table,
        fields: ctx.field_table.names,
        supertypes: grammar_json.supertypes.clone(),
        word_token: grammar_json.word.clone(),
        word_symbol_id,
        precedence_groups: precedence_groups_strs,
        conflict_decls: grammar_json.conflicts.clone(),
        reserved,
        dfa_table,
        line_start_offsets: Vec::new(),
    };

    Ok(grammar)
}

fn discover_symbols(rule: &Rule, ctx: &mut CompileCtx, externals: &[Rule]) {
    match rule {
        Rule::String { value } => {
            ctx.get_or_create_string(value);
        }
        Rule::Pattern { value, .. } => {
            ctx.get_or_create_pattern(value);
        }
        Rule::Symbol { name } => {
            if ctx.lookup(name).is_none()
                && !externals
                    .iter()
                    .any(|e| matches!(e, Rule::Symbol { name: n } if n == name))
            {
                ctx.get_or_create_string(name);
            }
        }
        Rule::Choice { members } | Rule::Seq { members } => {
            for m in members {
                discover_symbols(m, ctx, externals);
            }
        }
        Rule::Field { content, .. }
        | Rule::Alias { content, .. }
        | Rule::Token { content, .. }
        | Rule::ImmediateToken { content, .. }
        | Rule::Prec { content, .. }
        | Rule::PrecLeft { content, .. }
        | Rule::PrecRight { content, .. }
        | Rule::PrecDynamic { content, .. }
        | Rule::Repeat { content, .. }
        | Rule::Repeat1 { content, .. }
        | Rule::Reserved { content, .. } => {
            discover_symbols(content, ctx, externals);
        }
        Rule::Blank | Rule::Unknown(_) => {}
    }
}

fn build_dfa_table(symbols: &[Symbol]) -> DfaTable {
    let literals: Vec<(SymbolId, &[u8])> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Terminal)
        .filter_map(|s| {
            let name = s.name.trim_matches('"');
            if !name.is_empty() && !name.starts_with('/') {
                Some((s.id, name.as_bytes()))
            } else {
                None
            }
        })
        .collect();

    let patterns: Vec<(SymbolId, &str)> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Terminal)
        .filter_map(|s| {
            let name = s.name.as_str();
            if name.starts_with('/') && name.len() > 2 {
                let inner = &name[1..name.len() - 1];
                Some((s.id, inner))
            } else {
                None
            }
        })
        .collect();

    DfaTable::from_literals_and_patterns(&literals, &patterns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glr_core::StateId;

    #[test]
    fn compile_empty_grammar() {
        let json = r#"{"name":"empty","rules":{}}"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_trivial_grammar() {
        let json = r#"{
            "name": "trivial",
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "trivial grammar: {:?}", result.err());
        let grammar = result.unwrap();

        assert_eq!(grammar.symbol_count, 2);
        assert_eq!(grammar.productions.len(), 1);
        assert!(grammar.state_count > 0);
        assert!(grammar.parse_table.large_state_count > 0);
        assert!(!grammar.dfa_table.states.is_empty());
    }

    #[test]
    fn compile_choice_grammar() {
        let json = r#"{
            "name": "choice",
            "rules": {
                "value": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "STRING", "value": "true"},
                        {"type": "STRING", "value": "false"}
                    ]
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "choice: {:?}", result.err());
        let grammar = result.unwrap();
        assert_eq!(grammar.productions.len(), 2);
    }

    #[test]
    fn compile_seq_grammar() {
        let json = r#"{
            "name": "seq",
            "rules": {
                "pair": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "("},
                        {"type": "SYMBOL", "name": "value"},
                        {"type": "STRING", "value": ")"}
                    ]
                },
                "value": { "type": "STRING", "value": "x" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert!(grammar.state_count > 0);
    }

    #[test]
    fn compile_blank_grammar() {
        let json = r#"{
            "name": "epsilon",
            "rules": {
                "optional": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "STRING", "value": "a"},
                        {"type": "BLANK"}
                    ]
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "epsilon: {:?}", result.err());
        let grammar = result.unwrap();
        assert_eq!(grammar.productions.len(), 2);
    }

    #[test]
    fn compile_repeat_grammar() {
        let json = r#"{
            "name": "repeat_test",
            "rules": {
                "program": {
                    "type": "REPEAT",
                    "content": { "type": "STRING", "value": "a" }
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert!(grammar.productions.len() >= 2);
    }

    #[test]
    fn compile_repeat1_grammar() {
        let json = r#"{
            "name": "repeat1_test",
            "rules": {
                "program": {
                    "type": "REPEAT1",
                    "content": { "type": "STRING", "value": "a" }
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert!(grammar.productions.len() >= 2);
    }

    #[test]
    fn grammar_structurally_valid() {
        let json = r#"{
            "name": "test",
            "rules": {
                "expression": {
                    "type": "CHOICE",
                    "members": [
                        {
                            "type": "SEQ",
                            "members": [
                                {"type": "SYMBOL", "name": "expression"},
                                {"type": "STRING", "value": "+"},
                                {"type": "SYMBOL", "name": "expression"}
                            ]
                        },
                        {"type": "STRING", "value": "int"}
                    ]
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "grammar: {:?}", result.err());
        let grammar = result.unwrap();

        assert!(grammar.symbol_count > 0);
        assert!(grammar.token_count > 0);
        assert!(grammar.state_count > 0);
        assert!(!grammar.productions.is_empty());
        assert!(!grammar.dfa_table.states.is_empty());

        for prod in &grammar.productions {
            assert!(
                prod.nonterminal.0 < grammar.symbol_count,
                "nonterminal {} out of range",
                prod.nonterminal.0
            );
            for &sym in &prod.symbols {
                assert!(
                    sym.0 < grammar.symbol_count,
                    "symbol {} out of range",
                    sym.0
                );
            }
        }

        let table = &grammar.parse_table;
        assert!(
            !table.large_entries.is_empty() || !table.small_states.is_empty(),
            "parse table must have entries"
        );

        for tid in 0..grammar.token_count {
            let _actions = table.lookup(StateId(0), SymbolId(tid));
        }
    }

    #[test]
    fn dfa_contains_literals() {
        let json = r#"{
            "name": "keywords",
            "rules": {
                "keyword": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "STRING", "value": "if"},
                        {"type": "STRING", "value": "then"},
                        {"type": "STRING", "value": "else"}
                    ]
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert!(
            grammar.dfa_table.states.len() >= 4,
            "DFA should have at least 4 states"
        );
    }

    #[test]
    fn compile_with_extras() {
        let json = r#"{
            "name": "with_extras",
            "extras": [
                {"type": "STRING", "value": " "}
            ],
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_with_externals() {
        let json = r#"{
            "name": "with_externals",
            "externals": [
                {"type": "SYMBOL", "name": "indent"}
            ],
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert_eq!(grammar.external_token_count, 1);
    }

    #[test]
    fn compile_with_token_wrapper() {
        let json = r#"{
            "name": "token_test",
            "rules": {
                "tok": {
                    "type": "TOKEN",
                    "content": { "type": "STRING", "value": "keyword" }
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_with_field() {
        let json = r#"{
            "name": "field_test",
            "rules": {
                "node": {
                    "type": "FIELD",
                    "name": "child",
                    "content": { "type": "STRING", "value": "value" }
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert_eq!(grammar.field_count, 1);
        assert_eq!(grammar.fields, vec!["child"]);
    }

    #[test]
    fn compile_with_prec() {
        let json = r#"{
            "name": "prec_test",
            "rules": {
                "expr": {
                    "type": "PREC",
                    "value": 1,
                    "content": { "type": "STRING", "value": "term" }
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
    }

    #[test]
    fn symbol_ids_are_consistent() {
        let json = r#"{
            "name": "consistent",
            "rules": {
                "a": { "type": "SYMBOL", "name": "b" },
                "b": { "type": "STRING", "value": "x" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();

        for prod in &grammar.productions {
            for &sym_id in &prod.symbols {
                assert!(
                    (sym_id.0 as usize) < grammar.symbols.len(),
                    "symbol {} out of bounds",
                    sym_id.0
                );
            }
        }
    }

    #[test]
    fn compile_with_inline() {
        let json = r#"{
            "name": "inline_test",
            "inline": ["simple"],
            "rules": {
                "program": { "type": "SYMBOL", "name": "simple" },
                "simple": { "type": "STRING", "value": "x" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "inline grammar: {:?}", result.err());
        let grammar = result.unwrap();
        assert_eq!(grammar.symbol_count, 2);
        assert_eq!(grammar.productions.len(), 1);
    }

    #[test]
    fn compile_with_word() {
        let json = r#"{
            "name": "word_test",
            "word": "identifier",
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "word grammar: {:?}", result.err());
        let grammar = result.unwrap();
        assert_eq!(grammar.word_token, Some("identifier".to_string()));
    }

    #[test]
    fn compile_with_dynamic_precedence() {
        let json = r#"{
            "name": "dyn_prec",
            "rules": {
                "expr": {
                    "type": "PREC_DYNAMIC",
                    "value": 5,
                    "content": { "type": "STRING", "value": "x" }
                }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        for prod in &grammar.productions {
            assert_eq!(prod.dynamic_precedence, 5);
        }
    }

    #[test]
    fn compile_with_supertypes() {
        let json = r#"{
            "name": "super_test",
            "supertypes": ["expression", "statement"],
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert_eq!(grammar.supertypes, vec!["expression", "statement"]);
    }

    #[test]
    fn compile_with_precedence_groups() {
        let json = r#"{
            "name": "prec_groups",
            "precedences": [
                ["addition"],
                ["multiplication"]
            ],
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok(), "precedence groups: {:?}", result.err());
        let grammar = result.unwrap();
        assert_eq!(grammar.precedence_groups.len(), 2);
        assert_eq!(grammar.precedence_groups[0], vec!["addition"]);
        assert_eq!(grammar.precedence_groups[1], vec!["multiplication"]);
    }

    #[test]
    fn compile_with_conflicts() {
        let json = r#"{
            "name": "conflict_test",
            "conflicts": [
                ["expr", "pattern"]
            ],
            "rules": {
                "program": { "type": "STRING", "value": "hello" }
            }
        }"#;
        let result = compile_grammar(json);
        assert!(result.is_ok());
        let grammar = result.unwrap();
        assert_eq!(grammar.conflict_decls, vec![vec!["expr", "pattern"]]);
    }
}
