//! Semantic analysis phase: scope resolution (undefined variables, duplicate
//! declarations, invalid control flow) followed by static type checking
//! (type mismatches, bad operators, wrong argument types/counts).
//!
//! This runs entirely on the AST, after parsing and before IR generation.
//! It never executes the program; it only proves properties about it.

use crate::ast::*;
use crate::errors::{Diagnostic, DiagnosticBag};
use crate::types::{arith_result, compatible};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FuncSig {
    pub params: Vec<TypeAnnotation>,
    pub ret: TypeAnnotation,
}

#[derive(Debug, Clone, Default)]
pub struct ProgramInfo {
    pub functions: HashMap<String, FuncSig>,
    pub structs: HashMap<String, Vec<(String, TypeAnnotation)>>,
    pub enums: HashMap<String, Vec<String>>,
    pub variant_to_enum: HashMap<String, String>,
}

struct VarInfo {
    ty: TypeAnnotation,
    mutable: bool,
}

struct Scope {
    vars: HashMap<String, VarInfo>,
}

pub struct Analyzer<'a> {
    file: &'a str,
    info: ProgramInfo,
    scopes: Vec<Scope>,
    loop_depth: usize,
    current_return_type: Option<TypeAnnotation>,
    diagnostics: DiagnosticBag,
}

impl<'a> Analyzer<'a> {
    pub fn new(file: &'a str) -> Self {
        Analyzer {
            file,
            info: ProgramInfo::default(),
            scopes: vec![Scope {
                vars: HashMap::new(),
            }],
            loop_depth: 0,
            current_return_type: None,
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            vars: HashMap::new(),
        });
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(
        &mut self,
        name: &str,
        ty: TypeAnnotation,
        mutable: bool,
        _line: usize,
        _col: usize,
    ) {
        // EramLang explicitly supports shadowing (`let x = 10; let x = x + 20;`),
        // so redeclaring a name in the same scope simply rebinds it rather
        // than being a semantic error.
        let scope = self.scopes.last_mut().unwrap();
        scope.vars.insert(name.to_string(), VarInfo { ty, mutable });
    }

    fn lookup(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.vars.get(name) {
                return Some(v);
            }
        }
        None
    }

    fn err(&mut self, line: usize, col: usize, msg: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(self.file, line, col, msg.into()).with_code("E0003"));
    }

    pub fn analyze(mut self, program: &Program) -> Result<ProgramInfo, DiagnosticBag> {
        // Pass 1: hoist function/struct/enum signatures so forward calls work.
        for item in &program.items {
            match item {
                Item::Function(f) => {
                    let params = f.params.iter().map(|p| p.ty.clone()).collect();
                    if self.info.functions.contains_key(&f.name) {
                        self.err(f.line, f.col, format!("duplicate function `{}`", f.name));
                    }
                    self.info.functions.insert(
                        f.name.clone(),
                        FuncSig {
                            params,
                            ret: f.return_type.clone(),
                        },
                    );
                }
                Item::Struct(s) => {
                    if self.info.structs.contains_key(&s.name) {
                        self.err(s.line, s.col, format!("duplicate struct `{}`", s.name));
                    }
                    self.info.structs.insert(s.name.clone(), s.fields.clone());
                }
                Item::Enum(e) => {
                    if self.info.enums.contains_key(&e.name) {
                        self.err(e.line, e.col, format!("duplicate enum `{}`", e.name));
                    }
                    for v in &e.variants {
                        if let Some(existing) = self.info.variant_to_enum.get(v) {
                            self.err(
                                e.line,
                                e.col,
                                format!("enum variant `{}` already used by enum `{}`", v, existing),
                            );
                        }
                        self.info.variant_to_enum.insert(v.clone(), e.name.clone());
                    }
                    self.info.enums.insert(e.name.clone(), e.variants.clone());
                }
                Item::Statement(_) => {}
            }
        }

        // Standard-library function signatures (checked loosely: arity only
        // enforced for fixed-arity builtins).
        for (name, params, ret) in crate::stdlib::builtin_signatures() {
            self.info
                .functions
                .entry(name.to_string())
                .or_insert(FuncSig { params, ret });
        }

        // Pass 2: walk bodies.
        for item in &program.items {
            match item {
                Item::Function(f) => self.check_function(f),
                Item::Struct(_) | Item::Enum(_) => {}
                Item::Statement(s) => self.check_statement(s),
            }
        }

        if self.diagnostics.has_errors() {
            Err(self.diagnostics)
        } else {
            Ok(self.info)
        }
    }

    fn check_function(&mut self, f: &FunctionDecl) {
        self.push_scope();
        let prev_return = self.current_return_type.replace(f.return_type.clone());
        for p in &f.params {
            self.declare(&p.name, p.ty.clone(), false, f.line, f.col);
        }
        for stmt in &f.body {
            self.check_statement(stmt);
        }
        self.current_return_type = prev_return;
        self.pop_scope();
    }

    fn check_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Let {
                name,
                mutable,
                ty,
                value,
                line,
                col,
            } => {
                let value_ty = self.check_expr(value);
                let final_ty = if *ty == TypeAnnotation::Unknown {
                    value_ty.clone()
                } else {
                    if !compatible(ty, &value_ty) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                self.file,
                                *line,
                                *col,
                                format!("expected `{}`, found `{}`", ty, value_ty),
                            )
                            .with_code("E0021")
                            .with_help("change the annotation or the value to match"),
                        );
                    }
                    ty.clone()
                };
                self.declare(name, final_ty, *mutable, *line, *col);
            }
            Statement::Assign {
                target,
                value,
                line,
                col,
            } => {
                let value_ty = self.check_expr(value);
                self.check_assign_target(target, &value_ty, *line, *col);
            }
            Statement::CompoundAssign {
                target,
                value,
                line,
                col,
                ..
            } => {
                let value_ty = self.check_expr(value);
                self.check_assign_target(target, &value_ty, *line, *col);
            }
            Statement::ExprStmt(e) => {
                self.check_expr(e);
            }
            Statement::Return { value, line, col } => {
                let ret_ty = self
                    .current_return_type
                    .clone()
                    .unwrap_or(TypeAnnotation::Void);
                if self.current_return_type.is_none() {
                    self.err(*line, *col, "`return` used outside of a function");
                }
                let actual = match value {
                    Some(e) => self.check_expr(e),
                    None => TypeAnnotation::Void,
                };
                if !compatible(&ret_ty, &actual) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            self.file,
                            *line,
                            *col,
                            format!("expected return type `{}`, found `{}`", ret_ty, actual),
                        )
                        .with_code("E0022"),
                    );
                }
            }
            Statement::If {
                cond,
                then_block,
                else_block,
                line,
                col,
            } => {
                let cty = self.check_expr(cond);
                if !compatible(&cty, &TypeAnnotation::Bool) {
                    self.err(
                        *line,
                        *col,
                        format!("`if` condition must be `bool`, found `{}`", cty),
                    );
                }
                self.push_scope();
                for s in then_block {
                    self.check_statement(s);
                }
                self.pop_scope();
                if let Some(eb) = else_block {
                    self.push_scope();
                    for s in eb {
                        self.check_statement(s);
                    }
                    self.pop_scope();
                }
            }
            Statement::While {
                cond,
                body,
                line,
                col,
            } => {
                let cty = self.check_expr(cond);
                if !compatible(&cty, &TypeAnnotation::Bool) {
                    self.err(
                        *line,
                        *col,
                        format!("`while` condition must be `bool`, found `{}`", cty),
                    );
                }
                self.loop_depth += 1;
                self.push_scope();
                for s in body {
                    self.check_statement(s);
                }
                self.pop_scope();
                self.loop_depth -= 1;
            }
            Statement::For {
                var,
                start,
                end,
                body,
                ..
            } => {
                let sty = self.check_expr(start);
                let ety = self.check_expr(end);
                if !compatible(&sty, &TypeAnnotation::Int)
                    || !compatible(&ety, &TypeAnnotation::Int)
                {
                    self.err(0, 0, "`for` range bounds must be `int`");
                }
                self.loop_depth += 1;
                self.push_scope();
                self.declare(var, TypeAnnotation::Int, false, 0, 0);
                for s in body {
                    self.check_statement(s);
                }
                self.pop_scope();
                self.loop_depth -= 1;
            }
            Statement::Break { line, col } => {
                if self.loop_depth == 0 {
                    self.err(*line, *col, "`break` used outside of a loop");
                }
            }
            Statement::Continue { line, col } => {
                if self.loop_depth == 0 {
                    self.err(*line, *col, "`continue` used outside of a loop");
                }
            }
            Statement::Block(b) => {
                self.push_scope();
                for s in b {
                    self.check_statement(s);
                }
                self.pop_scope();
            }
        }
    }

    fn check_assign_target(
        &mut self,
        target: &Expr,
        value_ty: &TypeAnnotation,
        line: usize,
        col: usize,
    ) {
        match target {
            Expr::Identifier(name, l, c) => {
                if let Some(info) = self.lookup(name) {
                    if !info.mutable {
                        self.diagnostics.push(
                            Diagnostic::error(
                                self.file,
                                *l,
                                *c,
                                format!("cannot assign to immutable variable `{}`", name),
                            )
                            .with_code("E0004")
                            .with_help(format!("declare it as `let mut {}` instead", name)),
                        );
                    } else if !compatible(&info.ty, value_ty) {
                        self.diagnostics.push(Diagnostic::error(
                            self.file,
                            line,
                            col,
                            format!("expected `{}`, found `{}`", info.ty, value_ty),
                        ));
                    }
                } else {
                    self.err(*l, *c, format!("undefined variable `{}`", name));
                }
            }
            Expr::Index { array, .. } => {
                self.check_expr(array);
            }
            Expr::MemberAccess { object, .. } => {
                self.check_expr(object);
            }
            _ => {
                self.err(line, col, "invalid assignment target");
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> TypeAnnotation {
        match expr {
            Expr::Integer(..) => TypeAnnotation::Int,
            Expr::Float(..) => TypeAnnotation::Float,
            Expr::Bool(..) => TypeAnnotation::Bool,
            Expr::Str(..) => TypeAnnotation::Str,
            Expr::Char(..) => TypeAnnotation::Char,
            Expr::Identifier(name, line, col) => {
                if let Some(info) = self.lookup(name) {
                    info.ty.clone()
                } else if self.info.variant_to_enum.contains_key(name) {
                    TypeAnnotation::Named(self.info.variant_to_enum[name].clone())
                } else {
                    self.err(*line, *col, format!("undefined variable `{}`", name));
                    TypeAnnotation::Unknown
                }
            }
            Expr::Binary {
                left,
                op,
                right,
                line,
                col,
            } => {
                let lt = self.check_expr(left);
                let rt = self.check_expr(right);
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        match arith_result(*op == BinOp::Add, &lt, &rt) {
                            Some(t) => t,
                            None => {
                                self.err(
                                    *line,
                                    *col,
                                    format!(
                                        "invalid operand types `{}` and `{}` for operator",
                                        lt, rt
                                    ),
                                );
                                TypeAnnotation::Unknown
                            }
                        }
                    }
                    BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                        if !compatible(&lt, &rt) {
                            self.err(
                                *line,
                                *col,
                                format!("cannot compare `{}` with `{}`", lt, rt),
                            );
                        }
                        TypeAnnotation::Bool
                    }
                    BinOp::And | BinOp::Or => {
                        if !compatible(&lt, &TypeAnnotation::Bool)
                            || !compatible(&rt, &TypeAnnotation::Bool)
                        {
                            self.err(*line, *col, "logical operators require `bool` operands");
                        }
                        TypeAnnotation::Bool
                    }
                }
            }
            Expr::Unary {
                op,
                expr,
                line,
                col,
            } => {
                let t = self.check_expr(expr);
                match op {
                    UnOp::Neg => {
                        if !matches!(
                            t,
                            TypeAnnotation::Int | TypeAnnotation::Float | TypeAnnotation::Unknown
                        ) {
                            self.err(*line, *col, format!("cannot negate `{}`", t));
                        }
                        t
                    }
                    UnOp::Not => {
                        if !compatible(&t, &TypeAnnotation::Bool) {
                            self.err(*line, *col, format!("cannot apply `!` to `{}`", t));
                        }
                        TypeAnnotation::Bool
                    }
                }
            }
            Expr::Call {
                callee,
                args,
                line,
                col,
            } => {
                let arg_types: Vec<TypeAnnotation> =
                    args.iter().map(|a| self.check_expr(a)).collect();
                if let Some(sig) = self.info.functions.get(callee).cloned() {
                    if !sig.params.is_empty() || !crate::stdlib::is_variadic(callee) {
                        if sig.params.len() != arg_types.len()
                            && !crate::stdlib::is_variadic(callee)
                        {
                            self.err(
                                *line,
                                *col,
                                format!(
                                    "function `{}` expects {} argument(s), found {}",
                                    callee,
                                    sig.params.len(),
                                    arg_types.len()
                                ),
                            );
                        } else if !crate::stdlib::is_variadic(callee) {
                            for (i, (expected, actual)) in
                                sig.params.iter().zip(arg_types.iter()).enumerate()
                            {
                                if !compatible(expected, actual) {
                                    self.err(
                                        *line,
                                        *col,
                                        format!(
                                            "argument {} to `{}`: expected `{}`, found `{}`",
                                            i + 1,
                                            callee,
                                            expected,
                                            actual
                                        ),
                                    );
                                }
                            }
                        }
                    }
                    sig.ret
                } else {
                    self.err(
                        *line,
                        *col,
                        format!("call to undefined function `{}`", callee),
                    );
                    TypeAnnotation::Unknown
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                let mut elem_ty = TypeAnnotation::Unknown;
                for e in elements {
                    let t = self.check_expr(e);
                    if elem_ty == TypeAnnotation::Unknown {
                        elem_ty = t;
                    }
                }
                TypeAnnotation::Array(Box::new(elem_ty))
            }
            Expr::Index {
                array,
                index,
                line,
                col,
            } => {
                let at = self.check_expr(array);
                let it = self.check_expr(index);
                if !compatible(&it, &TypeAnnotation::Int) {
                    self.err(*line, *col, "array index must be `int`");
                }
                match at {
                    TypeAnnotation::Array(inner) => *inner,
                    TypeAnnotation::Unknown => TypeAnnotation::Unknown,
                    other => {
                        self.err(*line, *col, format!("cannot index into `{}`", other));
                        TypeAnnotation::Unknown
                    }
                }
            }
            Expr::StructLiteral {
                name,
                fields,
                line,
                col,
            } => {
                if let Some(decl_fields) = self.info.structs.get(name).cloned() {
                    for (fname, fexpr) in fields {
                        let fty = self.check_expr(fexpr);
                        if let Some((_, expected)) = decl_fields.iter().find(|(n, _)| n == fname) {
                            if !compatible(expected, &fty) {
                                self.err(
                                    *line,
                                    *col,
                                    format!(
                                        "field `{}` of `{}`: expected `{}`, found `{}`",
                                        fname, name, expected, fty
                                    ),
                                );
                            }
                        } else {
                            self.err(
                                *line,
                                *col,
                                format!("struct `{}` has no field `{}`", name, fname),
                            );
                        }
                    }
                } else {
                    self.err(*line, *col, format!("undefined struct `{}`", name));
                }
                TypeAnnotation::Named(name.clone())
            }
            Expr::MemberAccess {
                object,
                field,
                line,
                col,
            } => {
                let ot = self.check_expr(object);
                if let TypeAnnotation::Named(sname) = &ot {
                    if let Some(fields) = self.info.structs.get(sname) {
                        if let Some((_, fty)) = fields.iter().find(|(n, _)| n == field) {
                            return fty.clone();
                        } else {
                            self.err(
                                *line,
                                *col,
                                format!("struct `{}` has no field `{}`", sname, field),
                            );
                        }
                    }
                } else if ot != TypeAnnotation::Unknown {
                    self.err(*line, *col, format!("`{}` has no field `{}`", ot, field));
                }
                TypeAnnotation::Unknown
            }
            Expr::EnumValue {
                variant, line, col, ..
            } => {
                if let Some(ename) = self.info.variant_to_enum.get(variant) {
                    TypeAnnotation::Named(ename.clone())
                } else {
                    self.err(*line, *col, format!("undefined enum variant `{}`", variant));
                    TypeAnnotation::Unknown
                }
            }
            Expr::Match {
                subject,
                arms,
                line,
                col,
            } => {
                let sty = self.check_expr(subject);
                let enum_name = if let TypeAnnotation::Named(n) = &sty {
                    self.info.enums.get(n).cloned().map(|v| (n.clone(), v))
                } else {
                    None
                };
                if let Some((ename, variants)) = &enum_name {
                    for (variant, _) in arms {
                        if !variants.contains(variant) {
                            self.err(
                                *line,
                                *col,
                                format!("`{}` is not a variant of enum `{}`", variant, ename),
                            );
                        }
                    }
                    for v in variants {
                        if !arms.iter().any(|(a, _)| a == v) {
                            self.diagnostics.push(Diagnostic {
                                severity: crate::errors::Severity::Warning,
                                code: None,
                                message: format!(
                                    "match on `{}` does not cover variant `{}`",
                                    ename, v
                                ),
                                file: self.file.to_string(),
                                line: *line,
                                col: *col,
                                help: Some("add an arm for this variant".to_string()),
                            });
                        }
                    }
                } else if sty != TypeAnnotation::Unknown {
                    self.err(*line, *col, "`match` subject must be an enum value");
                }
                let mut result_ty = TypeAnnotation::Unknown;
                for (_, body) in arms {
                    let t = match body {
                        MatchArmBody::Expr(e) => self.check_expr(e),
                        MatchArmBody::Block(b) => {
                            self.push_scope();
                            let mut last = TypeAnnotation::Void;
                            for s in b {
                                if let Statement::ExprStmt(e) = s {
                                    last = self.check_expr(e);
                                } else {
                                    self.check_statement(s);
                                }
                            }
                            self.pop_scope();
                            last
                        }
                    };
                    if result_ty == TypeAnnotation::Unknown {
                        result_ty = t;
                    }
                }
                result_ty
            }
        }
    }
}
