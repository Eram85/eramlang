//! Hand-written recursive-descent parser producing an AST from a token
//! stream. Implements standard operator precedence via a precedence-climbing
//! expression parser.

use crate::ast::*;
use crate::errors::Diagnostic;
use crate::token::{Token, TokenKind};

pub type ParseResult<T> = Result<T, Diagnostic>;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    file: String,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, file: &str) -> Self {
        Parser {
            tokens,
            pos: 0,
            file: file.to_string(),
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn advance(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }

    fn matches(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> ParseResult<Token> {
        if self.check(&kind) {
            Ok(self.advance())
        } else {
            let t = self.peek().clone();
            Err(Diagnostic::error(
                &self.file,
                t.line,
                t.col,
                format!("expected {}, found {:?}", what, t.kind),
            )
            .with_code("E0001"))
        }
    }

    #[allow(dead_code)]
    fn err(&self, msg: impl Into<String>) -> Diagnostic {
        let t = self.peek();
        Diagnostic::error(&self.file, t.line, t.col, msg.into()).with_code("E0001")
    }

    // ---------- top level ----------

    pub fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    pub fn parse_program(&mut self) -> ParseResult<Program> {
        let mut items = Vec::new();
        while !self.check(&TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    pub fn parse_item(&mut self) -> ParseResult<Item> {
        match self.peek_kind() {
            TokenKind::Fn => Ok(Item::Function(self.parse_function()?)),
            TokenKind::Struct => Ok(Item::Struct(self.parse_struct()?)),
            TokenKind::Enum => Ok(Item::Enum(self.parse_enum()?)),
            _ => Ok(Item::Statement(self.parse_statement()?)),
        }
    }

    fn parse_type(&mut self) -> ParseResult<TypeAnnotation> {
        let t = self.advance();
        match t.kind {
            TokenKind::TypeInt => Ok(TypeAnnotation::Int),
            TokenKind::TypeFloat => Ok(TypeAnnotation::Float),
            TokenKind::TypeBool => Ok(TypeAnnotation::Bool),
            TokenKind::TypeString => Ok(TypeAnnotation::Str),
            TokenKind::TypeChar => Ok(TypeAnnotation::Char),
            TokenKind::TypeVoid => Ok(TypeAnnotation::Void),
            TokenKind::Identifier(name) => Ok(TypeAnnotation::Named(name)),
            TokenKind::LBracket => {
                let inner = self.parse_type()?;
                self.expect(TokenKind::RBracket, "']' to close array type")?;
                Ok(TypeAnnotation::Array(Box::new(inner)))
            }
            other => Err(Diagnostic::error(
                &self.file,
                t.line,
                t.col,
                format!("expected type, found {:?}", other),
            )),
        }
    }

    fn parse_function(&mut self) -> ParseResult<FunctionDecl> {
        let start = self.expect(TokenKind::Fn, "'fn'")?;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let pname = self.parse_ident()?;
                self.expect(TokenKind::Colon, "':' after parameter name")?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty });
                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "')'")?;
        let return_type = if self.matches(&TokenKind::Arrow) {
            self.parse_type()?
        } else {
            TypeAnnotation::Void
        };
        let body = self.parse_block()?;
        Ok(FunctionDecl {
            name,
            params,
            return_type,
            body,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_struct(&mut self) -> ParseResult<StructDecl> {
        let start = self.expect(TokenKind::Struct, "'struct'")?;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let fname = self.parse_ident()?;
            self.expect(TokenKind::Colon, "':' after field name")?;
            let ty = self.parse_type()?;
            fields.push((fname, ty));
            if !self.matches(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "'}'")?;
        Ok(StructDecl {
            name,
            fields,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_enum(&mut self) -> ParseResult<EnumDecl> {
        let start = self.expect(TokenKind::Enum, "'enum'")?;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            variants.push(self.parse_ident()?);
            if !self.matches(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "'}'")?;
        Ok(EnumDecl {
            name,
            variants,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_ident(&mut self) -> ParseResult<String> {
        let t = self.advance();
        match t.kind {
            TokenKind::Identifier(s) => Ok(s),
            other => Err(Diagnostic::error(
                &self.file,
                t.line,
                t.col,
                format!("expected identifier, found {:?}", other),
            )),
        }
    }

    // ---------- statements ----------

    fn parse_block(&mut self) -> ParseResult<Block> {
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            stmts.push(self.parse_statement()?);
        }
        self.expect(TokenKind::RBrace, "'}'")?;
        Ok(stmts)
    }

    fn parse_statement(&mut self) -> ParseResult<Statement> {
        match self.peek_kind().clone() {
            TokenKind::Let => self.parse_let(),
            TokenKind::Return => self.parse_return(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Break => {
                let t = self.advance();
                self.expect(TokenKind::Semicolon, "';' after 'break'")?;
                Ok(Statement::Break {
                    line: t.line,
                    col: t.col,
                })
            }
            TokenKind::Continue => {
                let t = self.advance();
                self.expect(TokenKind::Semicolon, "';' after 'continue'")?;
                Ok(Statement::Continue {
                    line: t.line,
                    col: t.col,
                })
            }
            TokenKind::LBrace => Ok(Statement::Block(self.parse_block()?)),
            _ => self.parse_expr_or_assign_statement(),
        }
    }

    fn parse_let(&mut self) -> ParseResult<Statement> {
        let start = self.expect(TokenKind::Let, "'let'")?;
        let mutable = self.matches(&TokenKind::Mut);
        let name = self.parse_ident()?;
        let ty = if self.matches(&TokenKind::Colon) {
            self.parse_type()?
        } else {
            TypeAnnotation::Unknown
        };
        self.expect(TokenKind::Eq, "'=' in let binding")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semicolon, "';' after let binding")?;
        Ok(Statement::Let {
            name,
            mutable,
            ty,
            value,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_return(&mut self) -> ParseResult<Statement> {
        let start = self.expect(TokenKind::Return, "'return'")?;
        let value = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::Semicolon, "';' after return")?;
        Ok(Statement::Return {
            value,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_if(&mut self) -> ParseResult<Statement> {
        let start = self.expect(TokenKind::If, "'if'")?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let else_block = if self.matches(&TokenKind::Else) {
            if self.check(&TokenKind::If) {
                let inner = self.parse_if()?;
                Some(vec![inner])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Statement::If {
            cond,
            then_block,
            else_block,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_while(&mut self) -> ParseResult<Statement> {
        let start = self.expect(TokenKind::While, "'while'")?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Statement::While {
            cond,
            body,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_for(&mut self) -> ParseResult<Statement> {
        let start = self.expect(TokenKind::For, "'for'")?;
        let var = self.parse_ident()?;
        self.expect(TokenKind::In, "'in'")?;
        let start_expr = self.parse_expr()?;
        self.expect(TokenKind::DotDot, "'..' in for-range")?;
        let end_expr = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Statement::For {
            var,
            start: start_expr,
            end: end_expr,
            body,
            line: start.line,
            col: start.col,
        })
    }

    fn parse_expr_or_assign_statement(&mut self) -> ParseResult<Statement> {
        let (line, col) = {
            let t = self.peek();
            (t.line, t.col)
        };
        let expr = self.parse_expr()?;

        // Assignment / compound assignment
        let compound_op = match self.peek_kind() {
            TokenKind::PlusEq => Some(BinOp::Add),
            TokenKind::MinusEq => Some(BinOp::Sub),
            TokenKind::StarEq => Some(BinOp::Mul),
            TokenKind::SlashEq => Some(BinOp::Div),
            _ => None,
        };

        if self.check(&TokenKind::Eq) {
            self.advance();
            let value = self.parse_expr()?;
            self.expect(TokenKind::Semicolon, "';' after assignment")?;
            return Ok(Statement::Assign {
                target: expr,
                value,
                line,
                col,
            });
        }
        if let Some(op) = compound_op {
            self.advance();
            let value = self.parse_expr()?;
            self.expect(TokenKind::Semicolon, "';' after assignment")?;
            return Ok(Statement::CompoundAssign {
                target: expr,
                op,
                value,
                line,
                col,
            });
        }

        self.expect(TokenKind::Semicolon, "';' after expression")?;
        Ok(Statement::ExprStmt(expr))
    }

    // ---------- expressions (precedence climbing) ----------

    pub fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_and()?;
        while self.check(&TokenKind::OrOr) {
            let t = self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
                line: t.line,
                col: t.col,
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_equality()?;
        while self.check(&TokenKind::AndAnd) {
            let t = self.advance();
            let right = self.parse_equality()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
                line: t.line,
                col: t.col,
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::NotEq => BinOp::Neq,
                _ => break,
            };
            let t = self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                line: t.line,
                col: t.col,
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::Le,
                TokenKind::GtEq => BinOp::Ge,
                _ => break,
            };
            let t = self.advance();
            let right = self.parse_additive()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                line: t.line,
                col: t.col,
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let t = self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                line: t.line,
                col: t.col,
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            let t = self.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                line: t.line,
                col: t.col,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        match self.peek_kind() {
            TokenKind::Minus => {
                let t = self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(expr),
                    line: t.line,
                    col: t.col,
                })
            }
            TokenKind::Bang => {
                let t = self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(expr),
                    line: t.line,
                    col: t.col,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek_kind() {
                TokenKind::LBracket => {
                    let t = self.advance();
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RBracket, "']' after index")?;
                    expr = Expr::Index {
                        array: Box::new(expr),
                        index: Box::new(index),
                        line: t.line,
                        col: t.col,
                    };
                }
                TokenKind::Dot => {
                    let t = self.advance();
                    let field = self.parse_ident()?;
                    expr = Expr::MemberAccess {
                        object: Box::new(expr),
                        field,
                        line: t.line,
                        col: t.col,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        let t = self.advance();
        match t.kind {
            TokenKind::Integer(v) => Ok(Expr::Integer(v, t.line, t.col)),
            TokenKind::Float(v) => Ok(Expr::Float(v, t.line, t.col)),
            TokenKind::True => Ok(Expr::Bool(true, t.line, t.col)),
            TokenKind::False => Ok(Expr::Bool(false, t.line, t.col)),
            TokenKind::StringLit(s) => Ok(Expr::Str(s, t.line, t.col)),
            TokenKind::CharLit(c) => Ok(Expr::Char(c, t.line, t.col)),
            TokenKind::LParen => {
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            TokenKind::LBracket => {
                let mut elements = Vec::new();
                if !self.check(&TokenKind::RBracket) {
                    loop {
                        elements.push(self.parse_expr()?);
                        if !self.matches(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RBracket, "']' to close array literal")?;
                Ok(Expr::ArrayLiteral {
                    elements,
                    line: t.line,
                    col: t.col,
                })
            }
            TokenKind::Match => {
                let subject = self.parse_expr()?;
                self.expect(TokenKind::LBrace, "'{' to open match body")?;
                let mut arms = Vec::new();
                while !self.check(&TokenKind::RBrace) {
                    let variant = self.parse_ident()?;
                    self.expect(TokenKind::FatArrow, "'=>' in match arm")?;
                    let body = if self.check(&TokenKind::LBrace) {
                        MatchArmBody::Block(self.parse_block()?)
                    } else {
                        MatchArmBody::Expr(Box::new(self.parse_expr()?))
                    };
                    arms.push((variant, body));
                    if !self.matches(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace, "'}' to close match body")?;
                Ok(Expr::Match {
                    subject: Box::new(subject),
                    arms,
                    line: t.line,
                    col: t.col,
                })
            }
            TokenKind::Identifier(name) => {
                if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if !self.matches(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen, "')' after arguments")?;
                    Ok(Expr::Call {
                        callee: name,
                        args,
                        line: t.line,
                        col: t.col,
                    })
                } else if self.check(&TokenKind::LBrace) && self.looks_like_struct_literal() {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RBrace) {
                        let fname = self.parse_ident()?;
                        self.expect(TokenKind::Colon, "':' in struct literal field")?;
                        let fval = self.parse_expr()?;
                        fields.push((fname, fval));
                        if !self.matches(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace, "'}' to close struct literal")?;
                    Ok(Expr::StructLiteral {
                        name,
                        fields,
                        line: t.line,
                        col: t.col,
                    })
                } else if self.check(&TokenKind::Colon) && self.peek_is_double_colon_like(&name) {
                    // Not used, reserved for future Enum::Variant syntax.
                    Ok(Expr::Identifier(name, t.line, t.col))
                } else {
                    Ok(Expr::Identifier(name, t.line, t.col))
                }
            }
            other => Err(Diagnostic::error(
                &self.file,
                t.line,
                t.col,
                format!("unexpected token {:?} in expression", other),
            )),
        }
    }

    fn peek_is_double_colon_like(&self, _name: &str) -> bool {
        false
    }

    /// Heuristic: `Identifier { ... }` is a struct literal, not a block,
    /// when used in expression position. We disambiguate by requiring the
    /// next tokens after `{` to look like `ident :` or an immediate `}`.
    fn looks_like_struct_literal(&self) -> bool {
        let mut i = self.pos + 1; // token after '{'
        if let Some(t) = self.tokens.get(i) {
            if matches!(t.kind, TokenKind::RBrace) {
                return true;
            }
            if let TokenKind::Identifier(_) = &t.kind {
                i += 1;
                if let Some(t2) = self.tokens.get(i) {
                    return matches!(t2.kind, TokenKind::Colon);
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Program {
        let tokens = Lexer::new(src, "<test>").tokenize().expect("lex ok");
        Parser::new(tokens, "<test>")
            .parse_program()
            .expect("parse ok")
    }

    #[test]
    fn test_parser_functions() {
        let prog = parse("fn add(a: int, b: int) -> int { return a + b; }");
        assert_eq!(prog.items.len(), 1);
        match &prog.items[0] {
            Item::Function(f) => {
                assert_eq!(f.name, "add");
                assert_eq!(f.params.len(), 2);
                assert_eq!(f.return_type, TypeAnnotation::Int);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parser_if() {
        let prog = parse("fn main() { if true { let x = 1; } else { let y = 2; } }");
        match &prog.items[0] {
            Item::Function(f) => match &f.body[0] {
                Statement::If { else_block, .. } => assert!(else_block.is_some()),
                _ => panic!("expected if statement"),
            },
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parser_precedence() {
        // 10 + 5 * 2 should parse as 10 + (5 * 2), i.e. the top-level
        // operator is Add with a Mul on the right.
        let prog = parse("let result = 10 + 5 * 2;");
        match &prog.items[0] {
            Item::Statement(Statement::Let { value, .. }) => match value {
                Expr::Binary { op, right, .. } => {
                    assert_eq!(*op, BinOp::Add);
                    assert!(matches!(**right, Expr::Binary { op: BinOp::Mul, .. }));
                }
                _ => panic!("expected binary expression"),
            },
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn test_parser_struct() {
        let prog = parse("struct User { name: string, age: int }");
        match &prog.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.name, "User");
                assert_eq!(s.fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parser_enum() {
        let prog = parse("enum Status { Success, Failure }");
        match &prog.items[0] {
            Item::Enum(e) => assert_eq!(e.variants, vec!["Success", "Failure"]),
            _ => panic!("expected enum"),
        }
    }

    #[test]
    fn test_parser_array_and_index() {
        let prog = parse("let a = [1, 2, 3]; let b = a[0];");
        assert_eq!(prog.items.len(), 2);
    }

    #[test]
    fn test_parser_for_loop() {
        let prog = parse("fn main() { for i in 0..10 { print(i); } }");
        match &prog.items[0] {
            Item::Function(f) => assert!(matches!(f.body[0], Statement::For { .. })),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parser_missing_semicolon_errors() {
        let tokens = Lexer::new("let x = 1", "<test>").tokenize().unwrap();
        let result = Parser::new(tokens, "<test>").parse_program();
        assert!(result.is_err());
    }
}
