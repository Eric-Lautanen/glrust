use super::query::{Capture, FieldConstraint, Pattern, Query};
use std::vec::Vec;

/// Compile a tree-sitter `.scm` query string into a `Query`.
///
/// Supports:
/// - `(node_kind)` – named node kind match
/// - `"anon"` – anonymous node match
/// - `(_)` – any named node
/// - `(*)` – any node
/// - `name: (child)` – field constraint
/// - `@capture` – capture annotation
/// - nested S-expressions
pub fn compile_query(source: &str) -> Result<Query, String> {
    let tokens = tokenize(source);
    let mut parser = Parser { tokens, pos: 0 };
    let mut captures: Vec<Capture> = Vec::new();
    let mut patterns: Vec<Pattern> = Vec::new();

    while parser.pos < parser.tokens.len() {
        if parser.pos >= parser.tokens.len() {
            break;
        }
        if parser.peek() == Some(&Token::LParen) {
            let mut pattern = parser.parse_pattern(&mut captures)?;
            // Handle optional capture annotation after closing paren: (expr) @name
            if parser.peek() == Some(&Token::At) {
                parser.advance();
                let name = match parser.advance() {
                    Some(Token::Ident(n)) | Some(Token::StringLit(n)) => n.clone(),
                    _ => return Err("expected capture name after '@'".into()),
                };
                let idx = captures
                    .iter()
                    .position(|c| c.name == name)
                    .unwrap_or_else(|| {
                        captures.push(Capture { name: name.clone() });
                        captures.len() - 1
                    });
                pattern.capture_index = Some(idx);
            }
            patterns.push(pattern);
        } else {
            break;
        }
    }

    Ok(Query { patterns, captures })
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    LParen,
    RParen,
    Colon,
    At,
    Ident(String),
    StringLit(String),
    Underscore,
    Star,
}

fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            ':' => { tokens.push(Token::Colon); chars.next(); }
            '@' => { tokens.push(Token::At); chars.next(); }
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '"' { chars.next(); break; }
                    s.push(c);
                    chars.next();
                }
                tokens.push(Token::StringLit(s));
            }
            '_' => { chars.next(); tokens.push(Token::Underscore); }
            '*' => { chars.next(); tokens.push(Token::Star); }
            c if c.is_whitespace() => { chars.next(); }
            c if is_ident_start(c) => {
                let mut s = String::new();
                s.push(c);
                chars.next();
                while let Some(&c2) = chars.peek() {
                    if is_ident_continue(c2) { s.push(c2); chars.next(); }
                    else { break; }
                }
                tokens.push(Token::Ident(s));
            }
            _ => { chars.next(); }
        }
    }
    tokens
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '.'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(t)
    }

    fn parse_pattern(&mut self, captures: &mut Vec<Capture>) -> Result<Pattern, String> {
        match self.advance() {
            Some(Token::LParen) => {}
            _ => return Err("expected '('".into()),
        }

        let (kind, named, is_anonymous) = match self.advance() {
            Some(Token::Ident(name)) => (Some(name.clone()), true, false),
            Some(Token::StringLit(name)) => (Some(name.clone()), false, true),
            Some(Token::Underscore) => (None, true, false),
            Some(Token::Star) => (None, false, false),
            _ => return Err("expected node kind, '_', '*', or string literal".into()),
        };

        let mut field_constraints: Vec<FieldConstraint> = Vec::new();
        let mut child_patterns: Vec<Pattern> = Vec::new();
        let mut capture_index: Option<usize> = None;

        loop {
            match self.peek() {
                Some(Token::RParen) => {
                    self.advance();
                    break;
                }
                Some(Token::At) => {
                    self.advance();
                    let name = match self.advance() {
                        Some(Token::Ident(n)) | Some(Token::StringLit(n)) => n.clone(),
                        _ => return Err("expected capture name after '@'".into()),
                    };
                    let idx = captures
                        .iter()
                        .position(|c| c.name == name)
                        .unwrap_or_else(|| {
                            captures.push(Capture { name: name.clone() });
                            captures.len() - 1
                        });
                    capture_index = Some(idx);
                }
                Some(Token::Ident(_))
                | Some(Token::StringLit(_))
                | Some(Token::Underscore)
                | Some(Token::Star)
                | Some(Token::LParen) => {
                    let saved = self.pos;
                    let tok = self.advance().cloned();
                    let is_field = matches!(self.peek(), Some(Token::Colon));

                    if is_field {
                        self.advance();
                        let field_name = match &tok {
                            Some(Token::Ident(n)) => n.clone(),
                            Some(Token::StringLit(n)) => n.clone(),
                            _ => return Err("expected field name".into()),
                        };
                        let inner = self.parse_pattern(captures)?;
                        field_constraints.push(FieldConstraint {
                            name: field_name,
                            pattern: Box::new(inner),
                        });
                    } else {
                        self.pos = saved;
                        let child = self.parse_pattern(captures)?;
                        child_patterns.push(child);
                    }
                }
                Some(Token::Colon) => {
                    self.advance();
                }
                None => {
                    return Err("unexpected end of input in pattern".into());
                }
            }
        }

        Ok(Pattern {
            kind,
            named,
            is_anonymous,
            field_constraints,
            child_patterns,
            capture_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_kind() {
        let q = compile_query("(identifier) @id").unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(q.patterns[0].kind.as_deref(), Some("identifier"));
        assert!(q.patterns[0].named);
        assert!(q.patterns[0].capture_index.is_some());
        assert_eq!(q.captures.len(), 1);
        assert_eq!(q.captures[0].name, "id");
    }

    #[test]
    fn test_wildcard() {
        let q = compile_query("(_) @a (*) @b").unwrap();
        assert_eq!(q.patterns.len(), 2);
        assert_eq!(q.patterns[0].kind, None);
        assert!(q.patterns[0].named);
        assert_eq!(q.patterns[1].kind, None);
        assert!(!q.patterns[1].named);
    }

    #[test]
    fn test_field_constraint() {
        let q = compile_query("(function name: (identifier) @n)").unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(q.patterns[0].kind.as_deref(), Some("function"));
        assert_eq!(q.patterns[0].field_constraints.len(), 1);
        assert_eq!(q.patterns[0].field_constraints[0].name, "name");
        assert_eq!(
            q.patterns[0].field_constraints[0].pattern.kind.as_deref(),
            Some("identifier")
        );
    }

    #[test]
    fn test_anonymous() {
        let q = compile_query("(\"return\") @ret").unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(q.patterns[0].kind.as_deref(), Some("return"));
        assert!(!q.patterns[0].named);
        assert!(q.patterns[0].is_anonymous);
    }

    #[test]
    fn test_nested_patterns() {
        let q = compile_query("(pair (identifier) (string))").unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(q.patterns[0].kind.as_deref(), Some("pair"));
        assert_eq!(q.patterns[0].child_patterns.len(), 2);
        assert_eq!(
            q.patterns[0].child_patterns[0].kind.as_deref(),
            Some("identifier")
        );
        assert_eq!(
            q.patterns[0].child_patterns[1].kind.as_deref(),
            Some("string")
        );
    }
}
