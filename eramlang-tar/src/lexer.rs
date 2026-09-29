//! Hand-written lexer that turns EramLang source text into a stream of
//! `Token`s. Tracks line/column for every token so later phases can emit
//! precise diagnostics.

use crate::errors::Diagnostic;
use crate::token::{keyword_lookup, Token, TokenKind};

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
    file: String,
}

pub type LexResult<T> = Result<T, Diagnostic>;

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str, file: &str) -> Self {
        Lexer {
            src: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            file: file.to_string(),
        }
    }

    fn peek(&self) -> u8 {
        *self.src.get(self.pos).unwrap_or(&0)
    }

    fn peek_at(&self, offset: usize) -> u8 {
        *self.src.get(self.pos + offset).unwrap_or(&0)
    }

    fn advance(&mut self) -> u8 {
        let c = self.peek();
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn skip_whitespace_and_comments(&mut self) -> LexResult<()> {
        loop {
            match self.peek() {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.advance();
                }
                b'/' if self.peek_at(1) == b'/' => {
                    while !self.at_end() && self.peek() != b'\n' {
                        self.advance();
                    }
                }
                b'/' if self.peek_at(1) == b'*' => {
                    let (start_line, start_col) = (self.line, self.col);
                    self.advance();
                    self.advance();
                    let mut closed = false;
                    while !self.at_end() {
                        if self.peek() == b'*' && self.peek_at(1) == b'/' {
                            self.advance();
                            self.advance();
                            closed = true;
                            break;
                        }
                        self.advance();
                    }
                    if !closed {
                        return Err(Diagnostic::error(
                            &self.file,
                            start_line,
                            start_col,
                            "unterminated block comment",
                        ));
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    pub fn tokenize(mut self) -> LexResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments()?;
            if self.at_end() {
                tokens.push(Token::new(TokenKind::Eof, self.line, self.col));
                break;
            }
            tokens.push(self.next_token()?);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> LexResult<Token> {
        let (line, col) = (self.line, self.col);
        let c = self.advance();

        macro_rules! tok {
            ($kind:expr) => {
                Ok(Token::new($kind, line, col))
            };
        }

        match c {
            b'(' => tok!(TokenKind::LParen),
            b')' => tok!(TokenKind::RParen),
            b'{' => tok!(TokenKind::LBrace),
            b'}' => tok!(TokenKind::RBrace),
            b'[' => tok!(TokenKind::LBracket),
            b']' => tok!(TokenKind::RBracket),
            b',' => tok!(TokenKind::Comma),
            b';' => tok!(TokenKind::Semicolon),
            b':' => tok!(TokenKind::Colon),
            b'.' => {
                if self.peek() == b'.' {
                    self.advance();
                    tok!(TokenKind::DotDot)
                } else {
                    tok!(TokenKind::Dot)
                }
            }
            b'+' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::PlusEq)
                } else {
                    tok!(TokenKind::Plus)
                }
            }
            b'-' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::MinusEq)
                } else if self.peek() == b'>' {
                    self.advance();
                    tok!(TokenKind::Arrow)
                } else {
                    tok!(TokenKind::Minus)
                }
            }
            b'*' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::StarEq)
                } else {
                    tok!(TokenKind::Star)
                }
            }
            b'/' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::SlashEq)
                } else {
                    tok!(TokenKind::Slash)
                }
            }
            b'%' => tok!(TokenKind::Percent),
            b'=' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::EqEq)
                } else if self.peek() == b'>' {
                    self.advance();
                    tok!(TokenKind::FatArrow)
                } else {
                    tok!(TokenKind::Eq)
                }
            }
            b'!' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::NotEq)
                } else {
                    tok!(TokenKind::Bang)
                }
            }
            b'<' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::LtEq)
                } else {
                    tok!(TokenKind::Lt)
                }
            }
            b'>' => {
                if self.peek() == b'=' {
                    self.advance();
                    tok!(TokenKind::GtEq)
                } else {
                    tok!(TokenKind::Gt)
                }
            }
            b'&' => {
                if self.peek() == b'&' {
                    self.advance();
                    tok!(TokenKind::AndAnd)
                } else {
                    Err(Diagnostic::error(
                        &self.file,
                        line,
                        col,
                        "unexpected character '&' (did you mean '&&'?)",
                    ))
                }
            }
            b'|' => {
                if self.peek() == b'|' {
                    self.advance();
                    tok!(TokenKind::OrOr)
                } else {
                    Err(Diagnostic::error(
                        &self.file,
                        line,
                        col,
                        "unexpected character '|' (did you mean '||'?)",
                    ))
                }
            }
            b'"' => self.lex_string(line, col),
            b'\'' => self.lex_char(line, col),
            b'0'..=b'9' => self.lex_number(line, col),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident(line, col),
            other => Err(Diagnostic::error(
                &self.file,
                line,
                col,
                format!("unexpected character '{}'", other as char),
            )),
        }
    }

    fn lex_string(&mut self, line: usize, col: usize) -> LexResult<Token> {
        let mut s = String::new();
        loop {
            if self.at_end() {
                return Err(Diagnostic::error(
                    &self.file,
                    line,
                    col,
                    "unterminated string literal",
                ));
            }
            let c = self.advance();
            if c == b'"' {
                break;
            }
            if c == b'\\' {
                let esc = self.advance();
                s.push(match esc {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'0' => '\0',
                    other => other as char,
                });
            } else {
                s.push(c as char);
            }
        }
        Ok(Token::new(TokenKind::StringLit(s), line, col))
    }

    fn lex_char(&mut self, line: usize, col: usize) -> LexResult<Token> {
        if self.at_end() {
            return Err(Diagnostic::error(
                &self.file,
                line,
                col,
                "unterminated character literal",
            ));
        }
        let c = self.advance();
        let ch = if c == b'\\' {
            let esc = self.advance();
            match esc {
                b'n' => '\n',
                b't' => '\t',
                b'r' => '\r',
                b'\\' => '\\',
                b'\'' => '\'',
                other => other as char,
            }
        } else {
            c as char
        };
        if self.peek() != b'\'' {
            return Err(Diagnostic::error(
                &self.file,
                line,
                col,
                "character literal must be a single character",
            ));
        }
        self.advance();
        Ok(Token::new(TokenKind::CharLit(ch), line, col))
    }

    fn lex_number(&mut self, line: usize, col: usize) -> LexResult<Token> {
        let start = self.pos - 1;
        while self.peek().is_ascii_digit() {
            self.advance();
        }
        let mut is_float = false;
        if self.peek() == b'.' && self.peek_at(1).is_ascii_digit() {
            is_float = true;
            self.advance();
            while self.peek().is_ascii_digit() {
                self.advance();
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        if is_float {
            let v: f64 = text.parse().map_err(|_| {
                Diagnostic::error(
                    &self.file,
                    line,
                    col,
                    format!("invalid float literal '{}'", text),
                )
            })?;
            Ok(Token::new(TokenKind::Float(v), line, col))
        } else {
            let v: i64 = text.parse().map_err(|_| {
                Diagnostic::error(
                    &self.file,
                    line,
                    col,
                    format!("invalid integer literal '{}'", text),
                )
            })?;
            Ok(Token::new(TokenKind::Integer(v), line, col))
        }
    }

    fn lex_ident(&mut self, line: usize, col: usize) -> LexResult<Token> {
        let start = self.pos - 1;
        while matches!(self.peek(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
            self.advance();
        }
        let text = std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .to_string();
        if let Some(kw) = keyword_lookup(&text) {
            Ok(Token::new(kw, line, col))
        } else {
            Ok(Token::new(TokenKind::Identifier(text), line, col))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;

    fn kinds(src: &str) -> Vec<TokenKind> {
        Lexer::new(src, "<test>")
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_lexer_numbers() {
        let k = kinds("10 20.5");
        assert_eq!(
            k,
            vec![
                TokenKind::Integer(10),
                TokenKind::Float(20.5),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_lexer_strings() {
        let k = kinds(r#""hello world""#);
        assert_eq!(
            k,
            vec![
                TokenKind::StringLit("hello world".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_lexer_string_escapes() {
        let k = kinds(r#""a\nb""#);
        assert_eq!(
            k,
            vec![TokenKind::StringLit("a\nb".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_lexer_keywords_vs_identifiers() {
        let k = kinds("let mut x fn foo");
        assert_eq!(
            k,
            vec![
                TokenKind::Let,
                TokenKind::Mut,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Fn,
                TokenKind::Identifier("foo".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_lexer_operators() {
        let k = kinds("+ - * / % == != <= >= && || += ->");
        assert_eq!(
            k,
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::AndAnd,
                TokenKind::OrOr,
                TokenKind::PlusEq,
                TokenKind::Arrow,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_lexer_comments_skipped() {
        let k = kinds("1 // comment\n2 /* block */ 3");
        assert_eq!(
            k,
            vec![
                TokenKind::Integer(1),
                TokenKind::Integer(2),
                TokenKind::Integer(3),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_lexer_unterminated_string_errors() {
        let result = Lexer::new("\"abc", "<test>").tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_lexer_tracks_line_and_column() {
        let tokens = Lexer::new("let\nx = 1;", "<test>").tokenize().unwrap();
        // `x` is on line 2.
        let x_tok = &tokens[1];
        assert_eq!(x_tok.line, 2);
    }
}
