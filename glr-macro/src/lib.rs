#![deny(unsafe_code)]
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{braced, bracketed, parse_macro_input, Token};

/// Native Rust DSL for defining a GLR grammar at compile time.
///
/// # Example
///
/// ```ignore
/// glr_grammar! {
///     language: "json",
///     tokens: {
///         "null" = "null",
///         "number" = r"\d+",
///         "string" = r#""[^"]*""#,
///     },
///     rules: {
///         value = { "null" } | { "number" } | { "string" },
///     }
/// }
/// ```
///
/// The grammar is compiled at compile time. The macro expands to an expression
/// that returns the compiled `(glr_core::Grammar, glr_lexer::DfaTable)` at runtime.
///
/// # Panics
///
/// Panics at compile time if the grammar compilation fails.
#[proc_macro]
pub fn glr_grammar(input: TokenStream) -> TokenStream {
    match parse_macro_input!(input as DslInput).to_json() {
        Ok(json_str) => {
            if let Err(e) = glr_grammar::compile_grammar(&json_str) {
                panic!("glr_grammar! compilation failed: {e}");
            }
            let expanded = quote! {{
                let __glr_json = #json_str;
                glr_grammar::compile_grammar(__glr_json)
                    .expect("glr_grammar! macro: grammar compilation failed")
            }};
            expanded.into()
        }
        Err(e) => {
            let msg = e.to_string();
            panic!("glr_grammar! parse error: {msg}");
        }
    }
}

// ---------------------------------------------------------------------------
// DSL Parsing
// ---------------------------------------------------------------------------

struct DslInput {
    language: Option<String>,
    word: Option<String>,
    extras: Vec<ExtraItem>,
    conflicts: Vec<Vec<String>>,
    precedences: Vec<Vec<String>>,
    tokens: Vec<TokenDef>,
    rules: Vec<RuleDef>,
}

enum ExtraItem {
    Lit(String),
    Pat(String),
}

impl ExtraItem {
    fn to_json(&self) -> String {
        match self {
            ExtraItem::Lit(s) => {
                format!("{{\"type\":\"STRING\",\"value\":{s}}}")
            }
            ExtraItem::Pat(s) => {
                format!("{{\"type\":\"PATTERN\",\"value\":{s}}}")
            }
        }
    }
}

struct TokenDef {
    name: String,
    value: TokenValue,
}

enum TokenValue {
    Lit(String),
    Pat(String),
}

impl TokenDef {
    fn to_json(&self) -> String {
        let value_json = match &self.value {
            TokenValue::Lit(s) => s.clone(),
            TokenValue::Pat(s) => s.clone(),
        };
        format!(
            "{{\"type\":\"{ty}\",\"value\":{val}}}",
            ty = match self.value {
                TokenValue::Lit(_) => "STRING",
                TokenValue::Pat(_) => "PATTERN",
            },
            val = value_json
        )
    }
}

struct RuleDef {
    name: String,
    alternatives: Vec<RuleAlt>,
}

struct RuleAlt {
    symbols: Vec<RuleSymbol>,
    attrs: Vec<Attr>,
}

enum RuleSymbol {
    Lit(String),
    NonTerm(String),
    Repeat(Box<RuleSymbol>),
    Repeat1(Box<RuleSymbol>),
}

struct Attr {
    kind: AttrKind,
    value: String,
}

enum AttrKind {
    Prec,
    PrecLeft,
    PrecRight,
    PrecDynamic,
}

impl RuleAlt {
    fn to_json(&self, is_last: bool) -> String {
        let mut result = if self.symbols.is_empty() {
            "{\"type\":\"BLANK\"}".to_string()
        } else if self.symbols.len() == 1 {
            self.symbols[0].to_json()
        } else {
            let members: Vec<String> = self.symbols.iter().map(|s| s.to_json()).collect();
            format!("{{\"type\":\"SEQ\",\"members\":[{}]}}", members.join(","))
        };

        for attr in &self.attrs {
            result = attr.wrap(result);
        }

        if !is_last {
            result.push(',');
        }
        result
    }
}

impl RuleSymbol {
    fn to_json(&self) -> String {
        match self {
            RuleSymbol::Lit(s) => format!("{{\"type\":\"STRING\",\"value\":{s}}}"),
            RuleSymbol::NonTerm(s) => format!("{{\"type\":\"SYMBOL\",\"name\":{s}}}"),
            RuleSymbol::Repeat(inner) => {
                format!("{{\"type\":\"REPEAT\",\"content\":{}}}", inner.to_json())
            }
            RuleSymbol::Repeat1(inner) => {
                format!("{{\"type\":\"REPEAT1\",\"content\":{}}}", inner.to_json())
            }
        }
    }
}

impl Attr {
    fn wrap(&self, inner: String) -> String {
        let value_str = &self.value;
        match self.kind {
            AttrKind::Prec => {
                format!("{{\"type\":\"PREC\",\"value\":{value_str},\"content\":{inner}}}")
            }
            AttrKind::PrecLeft => {
                format!("{{\"type\":\"PREC_LEFT\",\"value\":{value_str},\"content\":{inner}}}")
            }
            AttrKind::PrecRight => {
                format!("{{\"type\":\"PREC_RIGHT\",\"value\":{value_str},\"content\":{inner}}}")
            }
            AttrKind::PrecDynamic => {
                format!("{{\"type\":\"PREC_DYNAMIC\",\"value\":{value_str},\"content\":{inner}}}")
            }
        }
    }
}

impl DslInput {
    fn to_json(&self) -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(
            "\"name\":{}",
            self.language.as_deref().unwrap_or("\"grammar\"")
        ));

        if let Some(word) = &self.word {
            parts.push(format!("\"word\":{word}"));
        }

        if !self.extras.is_empty() {
            let extras: Vec<String> = self.extras.iter().map(|e| e.to_json()).collect();
            parts.push(format!("\"extras\":[{}]", extras.join(",")));
        }

        if !self.conflicts.is_empty() {
            let groups: Vec<String> = self
                .conflicts
                .iter()
                .map(|g| format!("[{}]", g.join(",")))
                .collect();
            parts.push(format!("\"conflicts\":[{}]", groups.join(",")));
        }

        if !self.precedences.is_empty() {
            let groups: Vec<String> = self
                .precedences
                .iter()
                .map(|g| {
                    let items: Vec<String> = g
                        .iter()
                        .map(|s| format!("{{\"type\":\"STRING\",\"value\":{s}}}"))
                        .collect();
                    format!("[{}]", items.join(","))
                })
                .collect();
            parts.push(format!("\"precedences\":[{}]", groups.join(",")));
        }

        if !self.tokens.is_empty() || !self.rules.is_empty() {
            let mut rules_map: Vec<String> = Vec::new();

            for tok in &self.tokens {
                let m = format!(
                    "\"{name}\":{val}",
                    name = tok.name.trim_matches('"'),
                    val = tok.to_json()
                );
                rules_map.push(m);
            }

            for rule in &self.rules {
                let alts: Vec<String> = rule
                    .alternatives
                    .iter()
                    .enumerate()
                    .map(|(i, a)| a.to_json(i == rule.alternatives.len() - 1))
                    .collect();
                let body = if alts.len() == 1 {
                    alts.into_iter().next().unwrap()
                } else {
                    format!("{{\"type\":\"CHOICE\",\"members\":[{}]}}", alts.join(","))
                };
                rules_map.push(format!("\"{name}\":{body}", name = rule.name));
            }

            parts.push(format!("\"rules\":{{{}}}", rules_map.join(",")));
        }

        Ok(format!("{{{}}}", parts.join(",")))
    }
}

// ---------------------------------------------------------------------------
// Syn Parse implementation
// ---------------------------------------------------------------------------

fn parse_string_or_regex(input: ParseStream) -> syn::Result<TokenValue> {
    if input.peek(syn::LitStr) {
        let lit: syn::LitStr = input.parse()?;
        Ok(TokenValue::Lit(format!("\"{}\"", lit.value())))
    } else if input.peek(Token![/]) {
        let _ = input.parse::<Token![/]>();
        let mut content = String::new();
        let mut depth = 0u32;
        while !input.peek(Token![/]) || depth > 0 {
            if input.is_empty() {
                return Err(input.error("unterminated regex pattern"));
            }
            let next: proc_macro2::TokenTree = input.parse()?;
            let s = next.to_string();
            if s == "(" || s == "[" || s == "{" {
                depth = depth.saturating_add(1);
            } else if s == ")" || s == "]" || s == "}" {
                depth = depth.saturating_sub(1);
            }
            content.push_str(&s);
        }
        let _ = input.parse::<Token![/]>();
        Ok(TokenValue::Pat(format!("\"{}\"", content.trim())))
    } else {
        Err(input.error(
            "expected string literal or regex pattern. Use r\"...\" for patterns with backslashes",
        ))
    }
}

impl Parse for ExtraItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::LitStr) {
            let lit: syn::LitStr = input.parse()?;
            Ok(ExtraItem::Lit(format!("\"{}\"", lit.value())))
        } else {
            let mut content = String::new();
            if input.peek(Token![/]) {
                let _ = input.parse::<Token![/]>();
            } else {
                return Err(input.error("expected string or /regex/"));
            }
            while !input.peek(Token![/]) && !input.is_empty() {
                let next: proc_macro2::TokenTree = input.parse()?;
                content.push_str(&next.to_string());
            }
            if input.peek(Token![/]) {
                let _ = input.parse::<Token![/]>();
            } else {
                return Err(input.error("unterminated regex"));
            }
            Ok(ExtraItem::Pat(format!("\"{}\"", content.trim())))
        }
    }
}

impl Parse for TokenDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name_lit: syn::LitStr = input.parse()?;
        let name = format!("\"{}\"", name_lit.value());
        input.parse::<Token![=]>()?;
        let value = parse_string_or_regex(input)?;
        Ok(TokenDef { name, value })
    }
}

impl Parse for RuleSymbol {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![*]) {
            input.parse::<Token![*]>()?;
            return Ok(RuleSymbol::Repeat(Box::new(RuleSymbol::NonTerm(
                "\"<any>\"".to_string(),
            ))));
        }
        if input.peek(syn::LitStr) {
            let lit: syn::LitStr = input.parse()?;
            Ok(RuleSymbol::Lit(format!("\"{}\"", lit.value())))
        } else if input.peek(Token![*]) {
            input.parse::<Token![*]>()?;
            Ok(RuleSymbol::NonTerm("\"<any>\"".to_string()))
        } else {
            let ident: syn::Ident = input.parse()?;
            let name = ident.to_string();
            if input.peek(Token![*]) {
                input.parse::<Token![*]>()?;
                Ok(RuleSymbol::Repeat(Box::new(RuleSymbol::NonTerm(format!(
                    "\"{name}\""
                )))))
            } else if input.peek(Token![+]) {
                input.parse::<Token![+]>()?;
                Ok(RuleSymbol::Repeat1(Box::new(RuleSymbol::NonTerm(format!(
                    "\"{name}\""
                )))))
            } else {
                Ok(RuleSymbol::NonTerm(format!("\"{name}\"")))
            }
        }
    }
}

impl Parse for Attr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<Token![#]>()?;
        let content;
        syn::bracketed!(content in input);
        let kind_str: syn::Ident = content.parse()?;
        let kind = match kind_str.to_string().as_str() {
            "prec" => AttrKind::Prec,
            "prec_left" => AttrKind::PrecLeft,
            "prec_right" => AttrKind::PrecRight,
            "prec_dynamic" => AttrKind::PrecDynamic,
            other => {
                return Err(content.error(format!("unknown attribute '{other}', expected prec, prec_left, prec_right, or prec_dynamic")));
            }
        };

        let value = if content.peek(syn::token::Paren) {
            let paren_content;
            syn::parenthesized!(paren_content in content);
            if paren_content.peek(syn::LitInt) {
                let int_lit: syn::LitInt = paren_content.parse()?;
                int_lit.base10_digits().to_string()
            } else {
                let lit: syn::LitStr = paren_content.parse()?;
                format!("\"{}\"", lit.value())
            }
        } else {
            let lit: syn::LitStr = content.parse()?;
            format!("\"{}\"", lit.value())
        };
        Ok(Attr { kind, value })
    }
}

impl Parse for RuleAlt {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let alt_content;
        braced!(alt_content in input);
        let mut symbols: Vec<RuleSymbol> = Vec::new();
        while !alt_content.is_empty() {
            if alt_content.peek(Token![,]) {
                let _ = alt_content.parse::<Token![,]>();
                break;
            }
            symbols.push(alt_content.parse()?);
            if alt_content.peek(Token![,]) {
                let _ = alt_content.parse::<Token![,]>();
            }
        }

        let mut attrs = Vec::new();
        while input.peek(Token![#]) {
            attrs.push(input.parse()?);
        }

        Ok(RuleAlt { symbols, attrs })
    }
}

impl Parse for RuleDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        let mut alternatives = Vec::new();
        alternatives.push(input.parse()?);

        while input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            alternatives.push(input.parse()?);
        }

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }

        Ok(RuleDef {
            name: name.to_string(),
            alternatives,
        })
    }
}

impl Parse for DslInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut language = None;
        let mut word = None;
        let mut extras = Vec::new();
        let mut conflicts = Vec::new();
        let mut precedences = Vec::new();
        let mut tokens = Vec::new();
        let mut rules = Vec::new();

        while !input.is_empty() {
            let field: syn::Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            match field.to_string().as_str() {
                "language" => {
                    let lit: syn::LitStr = input.parse()?;
                    language = Some(lit.value());
                }
                "word" => {
                    let lit: syn::LitStr = input.parse()?;
                    word = Some(format!("\"{}\"", lit.value()));
                }
                "extras" => {
                    let content;
                    bracketed!(content in input);
                    while !content.is_empty() {
                        extras.push(content.parse()?);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                "conflicts" => {
                    let content;
                    bracketed!(content in input);
                    while !content.is_empty() {
                        let group_content;
                        bracketed!(group_content in content);
                        let mut group = Vec::new();
                        while !group_content.is_empty() {
                            let lit: syn::LitStr = group_content.parse()?;
                            group.push(format!("\"{}\"", lit.value()));
                            if group_content.peek(Token![,]) {
                                group_content.parse::<Token![,]>()?;
                            }
                        }
                        conflicts.push(group);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                "precedences" => {
                    let content;
                    bracketed!(content in input);
                    while !content.is_empty() {
                        let group_content;
                        bracketed!(group_content in content);
                        let mut group = Vec::new();
                        while !group_content.is_empty() {
                            let lit: syn::LitStr = group_content.parse()?;
                            group.push(format!("\"{}\"", lit.value()));
                            if group_content.peek(Token![,]) {
                                group_content.parse::<Token![,]>()?;
                            }
                        }
                        precedences.push(group);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                "tokens" => {
                    let content;
                    braced!(content in input);
                    while !content.is_empty() {
                        tokens.push(content.parse()?);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                "rules" => {
                    let content;
                    braced!(content in input);
                    while !content.is_empty() {
                        rules.push(content.parse()?);
                    }
                }
                other => {
                    return Err(input.error(format!("unknown field '{other}'")));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(DslInput {
            language: language.map(|s| format!("\"{s}\"")),
            word,
            extras,
            conflicts,
            precedences,
            tokens,
            rules,
        })
    }
}
