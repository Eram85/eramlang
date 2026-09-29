//! Token types produced by the lexer.

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Integer(i64),
    Float(f64),
    StringLit(String),
    CharLit(char),
    Identifier(String),

    // Keywords
    Let,
    Mut,
    Fn,
    Return,
    If,
    Else,
    While,
    For,
    In,
    Break,
    Continue,
    Struct,
    Enum,
    Match,
    True,
    False,
    Print, // reserved but stdlib call is used in practice

    // Types (contextual identifiers are also allowed, but these are keywords)
    TypeInt,
    TypeFloat,
    TypeBool,
    TypeString,
    TypeChar,
    TypeVoid,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    AndAnd,
    OrOr,
    Bang,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Arrow,    // ->
    FatArrow, // =>
    DotDot,   // ..

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Colon,
    Dot,

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, col: usize) -> Self {
        Token { kind, line, col }
    }
}

pub fn keyword_lookup(ident: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match ident {
        "let" => Let,
        "mut" => Mut,
        "fn" => Fn,
        "return" => Return,
        "if" => If,
        "else" => Else,
        "while" => While,
        "for" => For,
        "in" => In,
        "break" => Break,
        "continue" => Continue,
        "struct" => Struct,
        "enum" => Enum,
        "match" => Match,
        "true" => True,
        "false" => False,
        "int" => TypeInt,
        "float" => TypeFloat,
        "bool" => TypeBool,
        "string" => TypeString,
        "char" => TypeChar,
        "void" => TypeVoid,
        _ => return None,
    })
}
