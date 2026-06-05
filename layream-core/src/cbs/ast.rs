//! AST for the CBS template language, plus the math sub-expression types.

use logos::Logos;

/// A structural node of a CBS template.
///
/// Only the *outer* structure is captured here. Tag arguments and block bodies
/// are kept as raw strings and re-evaluated during evaluation — this preserves
/// CBS's string-rewriting macro semantics, where `::` splitting and `{{slot}}`
/// substitution happen on raw text rather than on a structurally-parsed tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Literal text outside `{{ }}`.
    Text(String),
    /// A `{{ ... }}` tag; holds the trimmed inner content.
    Tag(String),
    /// A `{{#name ...}} ... {{/name}}` (or `{{:each ...}}`) block.
    Block {
        /// Raw opening-tag content, e.g. `#when::1::and::1`.
        header: String,
        /// Raw body text between the open and close tags, left unparsed.
        body: String,
    },
}

/// Math expression AST produced by the LALRPOP grammar (`grammar.lalrpop`).
#[derive(Debug, Clone, PartialEq)]
pub enum MathExpr {
    Num(f64),
    Neg(Box<MathExpr>),
    Bin(char, Box<MathExpr>, Box<MathExpr>),
}

impl MathExpr {
    pub fn neg(e: MathExpr) -> MathExpr {
        MathExpr::Neg(Box::new(e))
    }
    pub fn bin(op: char, l: MathExpr, r: MathExpr) -> MathExpr {
        MathExpr::Bin(op, Box::new(l), Box::new(r))
    }
}

/// Tokens for the math sub-language, lexed by logos.
///
/// Mirrors the legacy hand-written tokenizer: a numeric run (`[0-9.]+`) that
/// fails to parse yields a lex error, which the caller skips — matching the old
/// behavior of silently dropping unparseable runs.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
pub enum MathToken {
    #[regex(r"[0-9.]+", |lex| lex.slice().parse::<f64>().ok())]
    Num(f64),
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("^")]
    Caret,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
}
