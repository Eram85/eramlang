//! A small tree-walking interpreter used exclusively by the REPL.
//!
//! The "real" EramLang execution path is lex -> parse -> semantic analysis
//! -> IR -> bytecode -> VM (see `driver::compile` and `vm::Vm`), which is
//! what `eram run` / `eram build` use and what the test suite exercises.
//! The REPL needs incremental, statement-at-a-time evaluation against a
//! session that keeps growing (new `let`s, new `fn`s), which does not fit
//! the slot-indexed bytecode model (locals are resolved to fixed stack
//! slots at compile time for a whole program). Rather than recompiling and
//! *replaying* the entire session on every keystroke, the REPL walks the
//! AST directly against a persistent environment. It shares the lexer,
//! parser, AST and `Value` type with the rest of the compiler.

use crate::ast::*;
use crate::value::{StructValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub enum Flow {
    Normal(Value),
    Return(Value),
    Break,
    Continue,
}

#[derive(Default)]
pub struct ReplSession {
    pub functions: HashMap<String, FunctionDecl>,
    pub structs: HashMap<String, Vec<(String, TypeAnnotation)>>,
    pub variant_to_enum: HashMap<String, String>,
    pub globals: HashMap<String, Value>,
    pub mutable: HashMap<String, bool>,
}

type EvalResult = Result<Flow, String>;

impl ReplSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => {
                self.functions.insert(f.name.clone(), f.clone());
            }
            Item::Struct(s) => {
                self.structs.insert(s.name.clone(), s.fields.clone());
            }
            Item::Enum(e) => {
                for v in &e.variants {
                    self.variant_to_enum.insert(v.clone(), e.name.clone());
                }
            }
            Item::Statement(_) => {}
        }
    }

    /// Evaluates one top-level statement against the persistent session
    /// environment. Returns the value produced (Void for statements with
    /// no meaningful result) or an error message.
    pub fn eval_top_statement(&mut self, stmt: &Statement) -> Result<Value, String> {
        let mut locals: Vec<HashMap<String, Value>> = Vec::new();
        match self.eval_stmt(stmt, &mut locals)? {
            Flow::Normal(v) => Ok(v),
            Flow::Return(v) => Ok(v),
            Flow::Break | Flow::Continue => {
                Err("`break`/`continue` used outside of a loop".to_string())
            }
        }
    }

    fn lookup(&self, locals: &[HashMap<String, Value>], name: &str) -> Option<Value> {
        for scope in locals.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    fn assign(
        &mut self,
        locals: &mut [HashMap<String, Value>],
        name: &str,
        value: Value,
    ) -> Result<(), String> {
        for scope in locals.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        if self.globals.contains_key(name) {
            self.globals.insert(name.to_string(), value);
            Ok(())
        } else {
            Err(format!("undefined variable `{}`", name))
        }
    }

    fn declare(
        &mut self,
        locals: &mut Vec<HashMap<String, Value>>,
        name: &str,
        value: Value,
        mutable: bool,
    ) {
        if let Some(scope) = locals.last_mut() {
            scope.insert(name.to_string(), value);
        } else {
            self.globals.insert(name.to_string(), value);
            self.mutable.insert(name.to_string(), mutable);
        }
    }

    fn eval_block(
        &mut self,
        block: &[Statement],
        locals: &mut Vec<HashMap<String, Value>>,
    ) -> EvalResult {
        locals.push(HashMap::new());
        let mut last = Value::Void;
        for s in block {
            match self.eval_stmt(s, locals)? {
                Flow::Normal(v) => last = v,
                other => {
                    locals.pop();
                    return Ok(other);
                }
            }
        }
        locals.pop();
        Ok(Flow::Normal(last))
    }

    fn eval_stmt(
        &mut self,
        stmt: &Statement,
        locals: &mut Vec<HashMap<String, Value>>,
    ) -> EvalResult {
        match stmt {
            Statement::Let {
                name,
                mutable,
                value,
                ..
            } => {
                let v = self.eval_expr(value, locals)?;
                self.declare(locals, name, v, *mutable);
                Ok(Flow::Normal(Value::Void))
            }
            Statement::Assign { target, value, .. } => {
                let v = self.eval_expr(value, locals)?;
                self.assign_target(target, v, locals)?;
                Ok(Flow::Normal(Value::Void))
            }
            Statement::CompoundAssign {
                target, op, value, ..
            } => {
                let current = self.eval_expr(target, locals)?;
                let rhs = self.eval_expr(value, locals)?;
                let result = apply_binop(*op, &current, &rhs)?;
                self.assign_target(target, result, locals)?;
                Ok(Flow::Normal(Value::Void))
            }
            Statement::ExprStmt(e) => {
                let v = self.eval_expr(e, locals)?;
                Ok(Flow::Normal(v))
            }
            Statement::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.eval_expr(e, locals)?,
                    None => Value::Void,
                };
                Ok(Flow::Return(v))
            }
            Statement::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                let c = self.eval_expr(cond, locals)?;
                if c.is_truthy() {
                    self.eval_block(then_block, locals)
                } else if let Some(eb) = else_block {
                    self.eval_block(eb, locals)
                } else {
                    Ok(Flow::Normal(Value::Void))
                }
            }
            Statement::While { cond, body, .. } => {
                loop {
                    let c = self.eval_expr(cond, locals)?;
                    if !c.is_truthy() {
                        break;
                    }
                    match self.eval_block(body, locals)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal(_) => {}
                        r @ Flow::Return(_) => return Ok(r),
                    }
                }
                Ok(Flow::Normal(Value::Void))
            }
            Statement::For {
                var,
                start,
                end,
                body,
                ..
            } => {
                let s = self.eval_expr(start, locals)?;
                let e = self.eval_expr(end, locals)?;
                let (s, e) = match (s, e) {
                    (Value::Int(a), Value::Int(b)) => (a, b),
                    _ => return Err("`for` range bounds must be `int`".to_string()),
                };
                for i in s..e {
                    locals.push(HashMap::new());
                    locals
                        .last_mut()
                        .unwrap()
                        .insert(var.clone(), Value::Int(i));
                    let flow = self.eval_block(body, locals);
                    locals.pop();
                    match flow? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal(_) => {}
                        r @ Flow::Return(_) => return Ok(r),
                    }
                }
                Ok(Flow::Normal(Value::Void))
            }
            Statement::Break { .. } => Ok(Flow::Break),
            Statement::Continue { .. } => Ok(Flow::Continue),
            Statement::Block(b) => self.eval_block(b, locals),
        }
    }

    fn assign_target(
        &mut self,
        target: &Expr,
        value: Value,
        locals: &mut Vec<HashMap<String, Value>>,
    ) -> Result<(), String> {
        match target {
            Expr::Identifier(name, ..) => self.assign(locals, name, value),
            Expr::Index { array, index, .. } => {
                let arr = self.eval_expr(array, locals)?;
                let idx = self.eval_expr(index, locals)?;
                match (arr, idx) {
                    (Value::Array(a), Value::Int(i)) => {
                        let mut items = a.borrow_mut();
                        if i < 0 || i as usize >= items.len() {
                            return Err(format!(
                                "array index out of bounds: index {} for length {}",
                                i,
                                items.len()
                            ));
                        }
                        items[i as usize] = value;
                        Ok(())
                    }
                    _ => Err("invalid index assignment".to_string()),
                }
            }
            Expr::MemberAccess { object, field, .. } => {
                let obj = self.eval_expr(object, locals)?;
                match obj {
                    Value::Struct(s) => {
                        let mut fields = s.fields.borrow_mut();
                        match fields.iter_mut().find(|(n, _)| n == field) {
                            Some(slot) => {
                                slot.1 = value;
                                Ok(())
                            }
                            None => Err(format!("struct `{}` has no field `{}`", s.name, field)),
                        }
                    }
                    _ => Err("cannot assign to field of a non-struct value".to_string()),
                }
            }
            _ => Err("invalid assignment target".to_string()),
        }
    }

    fn eval_expr(
        &mut self,
        expr: &Expr,
        locals: &mut Vec<HashMap<String, Value>>,
    ) -> Result<Value, String> {
        match expr {
            Expr::Integer(v, ..) => Ok(Value::Int(*v)),
            Expr::Float(v, ..) => Ok(Value::Float(*v)),
            Expr::Bool(v, ..) => Ok(Value::Bool(*v)),
            Expr::Str(s, ..) => Ok(Value::Str(Rc::new(s.clone()))),
            Expr::Char(c, ..) => Ok(Value::Char(*c)),
            Expr::Identifier(name, ..) => {
                if let Some(v) = self.lookup(locals, name) {
                    Ok(v)
                } else if let Some(ename) = self.variant_to_enum.get(name) {
                    Ok(Value::Enum(Rc::new(ename.clone()), Rc::new(name.clone())))
                } else {
                    Err(format!("undefined variable `{}`", name))
                }
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                if *op == BinOp::And {
                    let l = self.eval_expr(left, locals)?;
                    if !l.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                    let r = self.eval_expr(right, locals)?;
                    return Ok(Value::Bool(r.is_truthy()));
                }
                if *op == BinOp::Or {
                    let l = self.eval_expr(left, locals)?;
                    if l.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                    let r = self.eval_expr(right, locals)?;
                    return Ok(Value::Bool(r.is_truthy()));
                }
                let l = self.eval_expr(left, locals)?;
                let r = self.eval_expr(right, locals)?;
                apply_binop(*op, &l, &r)
            }
            Expr::Unary { op, expr, .. } => {
                let v = self.eval_expr(expr, locals)?;
                match op {
                    UnOp::Neg => match v {
                        Value::Int(i) => Ok(Value::Int(-i)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        other => Err(format!("cannot negate `{}`", other.type_name())),
                    },
                    UnOp::Not => Ok(Value::Bool(!v.is_truthy())),
                }
            }
            Expr::Call { callee, args, .. } => {
                let mut arg_values = Vec::with_capacity(args.len());
                for a in args {
                    arg_values.push(self.eval_expr(a, locals)?);
                }
                self.call(callee, arg_values)
            }
            Expr::ArrayLiteral { elements, .. } => {
                let mut items = Vec::with_capacity(elements.len());
                for e in elements {
                    items.push(self.eval_expr(e, locals)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(items))))
            }
            Expr::Index { array, index, .. } => {
                let arr = self.eval_expr(array, locals)?;
                let idx = self.eval_expr(index, locals)?;
                match (arr, idx) {
                    (Value::Array(a), Value::Int(i)) => {
                        let items = a.borrow();
                        if i < 0 || i as usize >= items.len() {
                            return Err(format!(
                                "array index out of bounds: index {} for length {}",
                                i,
                                items.len()
                            ));
                        }
                        Ok(items[i as usize].clone())
                    }
                    (Value::Str(s), Value::Int(i)) => {
                        let chars: Vec<char> = s.chars().collect();
                        if i < 0 || i as usize >= chars.len() {
                            return Err("string index out of bounds".to_string());
                        }
                        Ok(Value::Char(chars[i as usize]))
                    }
                    (other, _) => Err(format!("cannot index `{}`", other.type_name())),
                }
            }
            Expr::StructLiteral { name, fields, .. } => {
                let mut values = Vec::with_capacity(fields.len());
                for (fname, fexpr) in fields {
                    values.push((fname.clone(), self.eval_expr(fexpr, locals)?));
                }
                Ok(Value::Struct(Rc::new(StructValue {
                    name: name.clone(),
                    fields: RefCell::new(values),
                })))
            }
            Expr::MemberAccess { object, field, .. } => {
                let obj = self.eval_expr(object, locals)?;
                match obj {
                    Value::Struct(s) => {
                        let fields = s.fields.borrow();
                        fields
                            .iter()
                            .find(|(n, _)| n == field)
                            .map(|(_, v)| v.clone())
                            .ok_or_else(|| format!("struct `{}` has no field `{}`", s.name, field))
                    }
                    other => Err(format!(
                        "cannot access field `{}` on `{}`",
                        field,
                        other.type_name()
                    )),
                }
            }
            Expr::EnumValue {
                enum_name, variant, ..
            } => {
                let ename = enum_name
                    .clone()
                    .or_else(|| self.variant_to_enum.get(variant).cloned())
                    .ok_or_else(|| format!("undefined enum variant `{}`", variant))?;
                Ok(Value::Enum(Rc::new(ename), Rc::new(variant.clone())))
            }
            Expr::Match { subject, arms, .. } => {
                let s = self.eval_expr(subject, locals)?;
                let variant_name = match &s {
                    Value::Enum(_, v) => v.to_string(),
                    other => {
                        return Err(format!(
                            "`match` subject must be an enum value, found `{}`",
                            other.type_name()
                        ))
                    }
                };
                for (variant, body) in arms {
                    if *variant == variant_name {
                        return match body {
                            MatchArmBody::Expr(e) => self.eval_expr(e, locals),
                            MatchArmBody::Block(b) => match self.eval_block(b, locals)? {
                                Flow::Normal(v) => Ok(v),
                                Flow::Return(v) => Ok(v),
                                _ => Ok(Value::Void),
                            },
                        };
                    }
                }
                Err(format!("no match arm for variant `{}`", variant_name))
            }
        }
    }

    fn call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        if crate::stdlib::is_builtin(name) {
            return crate::stdlib::call_builtin(name, args).map_err(|e| match e {
                crate::stdlib::RuntimeErr::Message(m) => m,
            });
        }
        let func = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| format!("call to undefined function `{}`", name))?;
        if func.params.len() != args.len() {
            return Err(format!(
                "function `{}` expects {} argument(s), found {}",
                name,
                func.params.len(),
                args.len()
            ));
        }
        let mut locals: Vec<HashMap<String, Value>> = vec![HashMap::new()];
        for (p, v) in func.params.iter().zip(args.into_iter()) {
            locals[0].insert(p.name.clone(), v);
        }
        match self.eval_block(&func.body, &mut locals)? {
            Flow::Return(v) => Ok(v),
            Flow::Normal(v) => Ok(v),
            _ => Ok(Value::Void),
        }
    }
}

fn apply_binop(op: BinOp, a: &Value, b: &Value) -> Result<Value, String> {
    use Value::*;
    match op {
        BinOp::Add => match (a, b) {
            (Int(x), Int(y)) => Ok(Int(x + y)),
            (Float(x), Float(y)) => Ok(Float(x + y)),
            (Int(x), Float(y)) => Ok(Float(*x as f64 + y)),
            (Float(x), Int(y)) => Ok(Float(x + *y as f64)),
            (Str(x), other) => Ok(Str(Rc::new(format!("{}{}", x, other)))),
            _ => Err(format!(
                "invalid operand types `{}` and `{}` for '+'",
                a.type_name(),
                b.type_name()
            )),
        },
        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            let (x, y) = match (a, b) {
                (Int(x), Int(y)) => (*x as f64, *y as f64),
                (Float(x), Float(y)) => (*x, *y),
                (Int(x), Float(y)) => (*x as f64, *y),
                (Float(x), Int(y)) => (*x, *y as f64),
                _ => {
                    return Err(format!(
                        "invalid operand types `{}` and `{}`",
                        a.type_name(),
                        b.type_name()
                    ))
                }
            };
            let both_int = matches!((a, b), (Int(_), Int(_)));
            let result = match op {
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => {
                    if y == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    x / y
                }
                BinOp::Mod => {
                    if y == 0.0 {
                        return Err("division by zero (in modulo)".to_string());
                    }
                    x % y
                }
                _ => unreachable!(),
            };
            if both_int {
                Ok(Int(result as i64))
            } else {
                Ok(Float(result))
            }
        }
        BinOp::Eq => Ok(Bool(values_equal(a, b))),
        BinOp::Neq => Ok(Bool(!values_equal(a, b))),
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
            let ord = match (a, b) {
                (Int(x), Int(y)) => x.partial_cmp(y),
                (Float(x), Float(y)) => x.partial_cmp(y),
                (Int(x), Float(y)) => (*x as f64).partial_cmp(y),
                (Float(x), Int(y)) => x.partial_cmp(&(*y as f64)),
                (Char(x), Char(y)) => x.partial_cmp(y),
                (Str(x), Str(y)) => x.partial_cmp(y),
                _ => {
                    return Err(format!(
                        "cannot compare `{}` with `{}`",
                        a.type_name(),
                        b.type_name()
                    ))
                }
            }
            .ok_or("comparison produced no ordering")?;
            use std::cmp::Ordering::*;
            Ok(Bool(match op {
                BinOp::Lt => ord == Less,
                BinOp::Gt => ord == Greater,
                BinOp::Le => ord != Greater,
                BinOp::Ge => ord != Less,
                _ => unreachable!(),
            }))
        }
        BinOp::And => Ok(Bool(a.is_truthy() && b.is_truthy())),
        BinOp::Or => Ok(Bool(a.is_truthy() || b.is_truthy())),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Enum(e1, v1), Value::Enum(e2, v2)) => e1 == e2 && v1 == v2,
        _ => false,
    }
}
