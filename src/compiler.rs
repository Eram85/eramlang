//! Two-stage code generation: `ast_to_ir` walks the AST and emits the
//! symbolic IR (`crate::ir`), then `ir_to_bytecode` lowers that IR into the
//! flat `bytecode::Program` the VM executes, resolving labels to jump
//! offsets and local names to stack slot indices along the way.

use crate::ast::*;
use crate::bytecode::{Function as BcFunction, Instr, Program as BcProgram};
use crate::ir::{IrFunction, IrOp, IrProgram};
use crate::semantic::ProgramInfo;
use crate::value::Value;
use std::collections::HashMap;

// ===================== Stage 1: AST -> IR =====================

pub struct IrGen<'a> {
    info: &'a ProgramInfo,
    label_counter: usize,
    temp_counter: usize,
    scopes: Vec<HashMap<String, String>>,
    loop_stack: Vec<(String, String)>, // (continue_label, break_label)
}

impl<'a> IrGen<'a> {
    pub fn new(info: &'a ProgramInfo) -> Self {
        IrGen {
            info,
            label_counter: 0,
            temp_counter: 0,
            scopes: Vec::new(),
            loop_stack: Vec::new(),
        }
    }

    fn fresh_label(&mut self, base: &str) -> String {
        self.label_counter += 1;
        format!("{}_{}", base, self.label_counter)
    }

    fn fresh_temp(&mut self) -> String {
        self.temp_counter += 1;
        format!("__t{}", self.temp_counter)
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str) -> String {
        self.temp_counter += 1;
        let unique = format!("{}${}", name, self.temp_counter);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), unique.clone());
        unique
    }

    fn resolve(&self, name: &str) -> String {
        for scope in self.scopes.iter().rev() {
            if let Some(u) = scope.get(name) {
                return u.clone();
            }
        }
        name.to_string()
    }

    pub fn gen_program(mut self, program: &Program) -> IrProgram {
        let mut functions = Vec::new();
        let mut top_level = Vec::new();
        self.push_scope(); // top-level scope for script-style `let`s
        let has_main = program
            .items
            .iter()
            .any(|i| matches!(i, Item::Function(f) if f.name == "main"));

        for item in &program.items {
            match item {
                Item::Function(f) => functions.push(self.gen_function(f)),
                Item::Struct(_) | Item::Enum(_) => {}
                Item::Statement(s) => self.gen_statement(s, &mut top_level),
            }
        }

        if has_main {
            top_level.push(IrOp::Call("main".to_string(), 0));
            top_level.push(IrOp::Pop);
        }

        IrProgram {
            functions,
            top_level,
            has_main,
        }
    }

    fn gen_function(&mut self, f: &FunctionDecl) -> IrFunction {
        self.push_scope();
        let mut params = Vec::new();
        for p in &f.params {
            params.push(self.declare(&p.name));
        }
        let mut body = Vec::new();
        for s in &f.body {
            self.gen_statement(s, &mut body);
        }
        body.push(IrOp::ConstVoid);
        body.push(IrOp::Return);
        self.pop_scope();
        IrFunction {
            name: f.name.clone(),
            params,
            body,
        }
    }

    fn gen_statement(&mut self, stmt: &Statement, out: &mut Vec<IrOp>) {
        match stmt {
            Statement::Let { name, value, .. } => {
                self.gen_expr(value, out);
                let unique = self.declare(name);
                out.push(IrOp::StoreLocal(unique));
            }
            Statement::Assign { target, value, .. } => {
                self.gen_expr(value, out);
                self.gen_assign_target(target, out);
            }
            Statement::CompoundAssign {
                target, op, value, ..
            } => {
                self.gen_load_target(target, out);
                self.gen_expr(value, out);
                out.push(binop_to_ir(*op));
                self.gen_assign_target(target, out);
            }
            Statement::ExprStmt(e) => {
                self.gen_expr(e, out);
                out.push(IrOp::Pop);
            }
            Statement::Return { value, .. } => match value {
                Some(e) => {
                    self.gen_expr(e, out);
                    out.push(IrOp::Return);
                }
                None => {
                    out.push(IrOp::ConstVoid);
                    out.push(IrOp::Return);
                }
            },
            Statement::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.gen_expr(cond, out);
                let else_label = self.fresh_label("else");
                let end_label = self.fresh_label("endif");
                out.push(IrOp::JumpIfFalse(else_label.clone()));
                self.push_scope();
                for s in then_block {
                    self.gen_statement(s, out);
                }
                self.pop_scope();
                out.push(IrOp::Jump(end_label.clone()));
                out.push(IrOp::Label(else_label));
                if let Some(eb) = else_block {
                    self.push_scope();
                    for s in eb {
                        self.gen_statement(s, out);
                    }
                    self.pop_scope();
                }
                out.push(IrOp::Label(end_label));
            }
            Statement::While { cond, body, .. } => {
                let start_label = self.fresh_label("while_start");
                let end_label = self.fresh_label("while_end");
                out.push(IrOp::Label(start_label.clone()));
                self.gen_expr(cond, out);
                out.push(IrOp::JumpIfFalse(end_label.clone()));
                self.loop_stack
                    .push((start_label.clone(), end_label.clone()));
                self.push_scope();
                for s in body {
                    self.gen_statement(s, out);
                }
                self.pop_scope();
                self.loop_stack.pop();
                out.push(IrOp::Jump(start_label));
                out.push(IrOp::Label(end_label));
            }
            Statement::For {
                var,
                start,
                end,
                body,
                ..
            } => {
                self.gen_expr(start, out);
                self.push_scope();
                let var_unique = self.declare(var);
                out.push(IrOp::StoreLocal(var_unique.clone()));
                self.gen_expr(end, out);
                let end_tmp = self.fresh_temp();
                out.push(IrOp::StoreLocal(end_tmp.clone()));

                let start_label = self.fresh_label("for_start");
                let continue_label = self.fresh_label("for_continue");
                let end_label = self.fresh_label("for_end");

                out.push(IrOp::Label(start_label.clone()));
                out.push(IrOp::LoadLocal(var_unique.clone()));
                out.push(IrOp::LoadLocal(end_tmp.clone()));
                out.push(IrOp::Lt);
                out.push(IrOp::JumpIfFalse(end_label.clone()));

                self.loop_stack
                    .push((continue_label.clone(), end_label.clone()));
                for s in body {
                    self.gen_statement(s, out);
                }
                self.loop_stack.pop();

                out.push(IrOp::Label(continue_label));
                out.push(IrOp::LoadLocal(var_unique.clone()));
                out.push(IrOp::ConstInt(1));
                out.push(IrOp::Add);
                out.push(IrOp::StoreLocal(var_unique));
                out.push(IrOp::Jump(start_label));
                out.push(IrOp::Label(end_label));
                self.pop_scope();
            }
            Statement::Break { .. } => {
                if let Some((_, brk)) = self.loop_stack.last().cloned() {
                    out.push(IrOp::Jump(brk));
                }
            }
            Statement::Continue { .. } => {
                if let Some((cont, _)) = self.loop_stack.last().cloned() {
                    out.push(IrOp::Jump(cont));
                }
            }
            Statement::Block(b) => {
                self.push_scope();
                for s in b {
                    self.gen_statement(s, out);
                }
                self.pop_scope();
            }
        }
    }

    fn gen_load_target(&mut self, target: &Expr, out: &mut Vec<IrOp>) {
        match target {
            Expr::Identifier(name, ..) => out.push(IrOp::LoadLocal(self.resolve(name))),
            Expr::Index { array, index, .. } => {
                self.gen_expr(array, out);
                self.gen_expr(index, out);
                out.push(IrOp::IndexGet);
            }
            Expr::MemberAccess { object, field, .. } => {
                self.gen_expr(object, out);
                out.push(IrOp::GetField(field.clone()));
            }
            _ => {}
        }
    }

    /// Precondition: the value to store is already on top of the IR stack.
    fn gen_assign_target(&mut self, target: &Expr, out: &mut Vec<IrOp>) {
        match target {
            Expr::Identifier(name, ..) => {
                let r = self.resolve(name);
                out.push(IrOp::StoreLocal(r));
            }
            Expr::Index { array, index, .. } => {
                let val_tmp = self.fresh_temp();
                out.push(IrOp::StoreLocal(val_tmp.clone()));
                self.gen_expr(array, out);
                self.gen_expr(index, out);
                out.push(IrOp::LoadLocal(val_tmp));
                out.push(IrOp::IndexSet);
            }
            Expr::MemberAccess { object, field, .. } => {
                let val_tmp = self.fresh_temp();
                out.push(IrOp::StoreLocal(val_tmp.clone()));
                self.gen_expr(object, out);
                out.push(IrOp::LoadLocal(val_tmp));
                out.push(IrOp::SetField(field.clone()));
            }
            _ => {}
        }
    }

    fn gen_expr(&mut self, expr: &Expr, out: &mut Vec<IrOp>) {
        match expr {
            Expr::Integer(v, ..) => out.push(IrOp::ConstInt(*v)),
            Expr::Float(v, ..) => out.push(IrOp::ConstFloat(*v)),
            Expr::Bool(v, ..) => out.push(IrOp::ConstBool(*v)),
            Expr::Str(s, ..) => out.push(IrOp::ConstStr(s.clone())),
            Expr::Char(c, ..) => out.push(IrOp::ConstChar(*c)),
            Expr::Identifier(name, ..) => {
                let resolved = self.resolve(name);
                if resolved != *name || self.is_declared(name) {
                    out.push(IrOp::LoadLocal(resolved));
                } else if let Some(ename) = self.info.variant_to_enum.get(name) {
                    out.push(IrOp::MakeEnum(ename.clone(), name.clone()));
                } else {
                    out.push(IrOp::LoadLocal(resolved));
                }
            }
            Expr::Binary {
                left, op, right, ..
            } => match op {
                BinOp::And => {
                    self.gen_expr(left, out);
                    let false_label = self.fresh_label("and_false");
                    let end_label = self.fresh_label("and_end");
                    out.push(IrOp::JumpIfFalse(false_label.clone()));
                    self.gen_expr(right, out);
                    out.push(IrOp::Jump(end_label.clone()));
                    out.push(IrOp::Label(false_label));
                    out.push(IrOp::ConstBool(false));
                    out.push(IrOp::Label(end_label));
                }
                BinOp::Or => {
                    self.gen_expr(left, out);
                    let true_label = self.fresh_label("or_true");
                    let end_label = self.fresh_label("or_end");
                    out.push(IrOp::JumpIfTrue(true_label.clone()));
                    self.gen_expr(right, out);
                    out.push(IrOp::Jump(end_label.clone()));
                    out.push(IrOp::Label(true_label));
                    out.push(IrOp::ConstBool(true));
                    out.push(IrOp::Label(end_label));
                }
                _ => {
                    self.gen_expr(left, out);
                    self.gen_expr(right, out);
                    out.push(binop_to_ir(*op));
                }
            },
            Expr::Unary { op, expr, .. } => {
                self.gen_expr(expr, out);
                out.push(match op {
                    UnOp::Neg => IrOp::Neg,
                    UnOp::Not => IrOp::Not,
                });
            }
            Expr::Call { callee, args, .. } => {
                for a in args {
                    self.gen_expr(a, out);
                }
                if crate::stdlib::is_builtin(callee) {
                    out.push(IrOp::CallBuiltin(callee.clone(), args.len()));
                } else {
                    out.push(IrOp::Call(callee.clone(), args.len()));
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                for e in elements {
                    self.gen_expr(e, out);
                }
                out.push(IrOp::MakeArray(elements.len()));
            }
            Expr::Index { array, index, .. } => {
                self.gen_expr(array, out);
                self.gen_expr(index, out);
                out.push(IrOp::IndexGet);
            }
            Expr::StructLiteral { name, fields, .. } => {
                let mut field_names = Vec::new();
                for (fname, fexpr) in fields {
                    self.gen_expr(fexpr, out);
                    field_names.push(fname.clone());
                }
                out.push(IrOp::MakeStruct(name.clone(), field_names));
            }
            Expr::MemberAccess { object, field, .. } => {
                self.gen_expr(object, out);
                out.push(IrOp::GetField(field.clone()));
            }
            Expr::EnumValue {
                enum_name, variant, ..
            } => {
                let ename = enum_name
                    .clone()
                    .or_else(|| self.info.variant_to_enum.get(variant).cloned())
                    .unwrap_or_default();
                out.push(IrOp::MakeEnum(ename, variant.clone()));
            }
            Expr::Match { subject, arms, .. } => {
                self.gen_expr(subject, out);
                let subj_tmp = self.fresh_temp();
                out.push(IrOp::StoreLocal(subj_tmp.clone()));
                let end_label = self.fresh_label("match_end");
                let enum_name = arms
                    .first()
                    .and_then(|(v, _)| self.info.variant_to_enum.get(v).cloned())
                    .unwrap_or_default();

                for (variant, body) in arms {
                    let next_label = self.fresh_label("match_arm");
                    out.push(IrOp::LoadLocal(subj_tmp.clone()));
                    out.push(IrOp::MakeEnum(enum_name.clone(), variant.clone()));
                    out.push(IrOp::Eq);
                    out.push(IrOp::JumpIfFalse(next_label.clone()));
                    match body {
                        MatchArmBody::Expr(e) => self.gen_expr(e, out),
                        MatchArmBody::Block(b) => {
                            self.push_scope();
                            if b.is_empty() {
                                out.push(IrOp::ConstVoid);
                            }
                            for (i, s) in b.iter().enumerate() {
                                if i == b.len() - 1 {
                                    if let Statement::ExprStmt(e) = s {
                                        self.gen_expr(e, out);
                                    } else {
                                        self.gen_statement(s, out);
                                        out.push(IrOp::ConstVoid);
                                    }
                                } else {
                                    self.gen_statement(s, out);
                                }
                            }
                            self.pop_scope();
                        }
                    }
                    out.push(IrOp::Jump(end_label.clone()));
                    out.push(IrOp::Label(next_label));
                }
                out.push(IrOp::ConstVoid);
                out.push(IrOp::Label(end_label));
            }
        }
    }

    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|s| s.contains_key(name))
    }
}

fn binop_to_ir(op: BinOp) -> IrOp {
    match op {
        BinOp::Add => IrOp::Add,
        BinOp::Sub => IrOp::Sub,
        BinOp::Mul => IrOp::Mul,
        BinOp::Div => IrOp::Div,
        BinOp::Mod => IrOp::Mod,
        BinOp::Eq => IrOp::Eq,
        BinOp::Neq => IrOp::Neq,
        BinOp::Lt => IrOp::Lt,
        BinOp::Gt => IrOp::Gt,
        BinOp::Le => IrOp::Le,
        BinOp::Ge => IrOp::Ge,
        BinOp::And => IrOp::And,
        BinOp::Or => IrOp::Or,
    }
}

// ===================== Stage 2: IR -> Bytecode =====================

pub struct CodeGen;

impl CodeGen {
    pub fn lower(program: IrProgram) -> BcProgram {
        let mut functions = Vec::new();
        for f in &program.functions {
            functions.push(Self::lower_function(&f.name, &f.params, &f.body));
        }
        let main_index = if program.has_main {
            functions.iter().position(|f| f.name == "__script__")
        } else {
            None
        };
        let script = Self::lower_function("__script__", &[], &program.top_level);
        functions.push(script);
        let script_index = functions.len() - 1;

        BcProgram {
            functions,
            main_index: Some(script_index).or(main_index),
            num_globals: 0,
            global_names: Vec::new(),
        }
    }

    fn lower_function(name: &str, params: &[String], body: &[IrOp]) -> BcFunction {
        // First pass: find label offsets in the *final* instruction stream.
        let mut label_pos: HashMap<String, usize> = HashMap::new();
        let mut linear: Vec<&IrOp> = Vec::new();
        for op in body {
            if let IrOp::Label(name) = op {
                label_pos.insert(name.clone(), linear.len());
            } else {
                linear.push(op);
            }
        }

        let mut slots: HashMap<String, usize> = HashMap::new();
        for p in params {
            let idx = slots.len();
            slots.insert(p.clone(), idx);
        }

        let mut constants: Vec<Value> = Vec::new();
        let mut code: Vec<Instr> = Vec::new();
        let mut lines: Vec<usize> = Vec::new();

        let slot_of = |name: &str, slots: &mut HashMap<String, usize>| -> usize {
            if let Some(i) = slots.get(name) {
                *i
            } else {
                let i = slots.len();
                slots.insert(name.to_string(), i);
                i
            }
        };

        for op in &linear {
            let instr = match op {
                IrOp::ConstInt(v) => {
                    constants.push(Value::Int(*v));
                    Instr::Const(constants.len() - 1)
                }
                IrOp::ConstFloat(v) => {
                    constants.push(Value::Float(*v));
                    Instr::Const(constants.len() - 1)
                }
                IrOp::ConstBool(v) => {
                    constants.push(Value::Bool(*v));
                    Instr::Const(constants.len() - 1)
                }
                IrOp::ConstStr(v) => {
                    constants.push(Value::Str(std::rc::Rc::new(v.clone())));
                    Instr::Const(constants.len() - 1)
                }
                IrOp::ConstChar(v) => {
                    constants.push(Value::Char(*v));
                    Instr::Const(constants.len() - 1)
                }
                IrOp::ConstVoid => Instr::ConstVoid,
                IrOp::Pop => Instr::Pop,
                IrOp::Add => Instr::Add,
                IrOp::Sub => Instr::Sub,
                IrOp::Mul => Instr::Mul,
                IrOp::Div => Instr::Div,
                IrOp::Mod => Instr::Mod,
                IrOp::Neg => Instr::Neg,
                IrOp::Not => Instr::Not,
                IrOp::Eq => Instr::Eq,
                IrOp::Neq => Instr::Neq,
                IrOp::Lt => Instr::Lt,
                IrOp::Gt => Instr::Gt,
                IrOp::Le => Instr::Le,
                IrOp::Ge => Instr::Ge,
                IrOp::And => Instr::And,
                IrOp::Or => Instr::Or,
                IrOp::LoadLocal(n) => Instr::LoadLocal(slot_of(n, &mut slots)),
                IrOp::StoreLocal(n) => Instr::StoreLocal(slot_of(n, &mut slots)),
                IrOp::LoadGlobal(_) | IrOp::StoreGlobal(_) => unreachable!("globals unused"),
                IrOp::Label(_) => unreachable!("labels removed in first pass"),
                IrOp::Jump(l) => {
                    Instr::Jump(0isize.wrapping_add(*label_pos.get(l).unwrap() as isize))
                }
                IrOp::JumpIfFalse(l) => Instr::JumpIfFalse(*label_pos.get(l).unwrap() as isize),
                IrOp::JumpIfTrue(l) => Instr::JumpIfTrue(*label_pos.get(l).unwrap() as isize),
                IrOp::Call(name, argc) => Instr::CallBuiltin(format!("__fn__{}", name), *argc), // placeholder, fixed below
                IrOp::CallBuiltin(name, argc) => Instr::CallBuiltin(name.clone(), *argc),
                IrOp::Return => Instr::Return,
                IrOp::ReturnVoid => Instr::ReturnVoid,
                IrOp::MakeArray(n) => Instr::MakeArray(*n),
                IrOp::IndexGet => Instr::IndexGet,
                IrOp::IndexSet => Instr::IndexSet,
                IrOp::MakeStruct(n, fields) => Instr::MakeStruct(n.clone(), fields.clone()),
                IrOp::GetField(n) => Instr::GetField(n.clone()),
                IrOp::SetField(n) => Instr::SetField(n.clone()),
                IrOp::MakeEnum(e, v) => Instr::MakeEnum(e.clone(), v.clone()),
                IrOp::Print(nl) => Instr::Print(*nl),
            };
            code.push(instr);
            lines.push(0);
        }

        let mut local_names = vec![String::new(); slots.len()];
        for (name, idx) in &slots {
            local_names[*idx] = name.clone();
        }

        BcFunction {
            name: name.to_string(),
            arity: params.len(),
            num_locals: slots.len(),
            constants,
            code,
            lines,
            local_names,
        }
    }
}

/// Second pass over already-lowered functions: rewrite the placeholder
/// `Call` markers into real `Instr::Call(function_index, argc)` once every
/// function's index in the program table is known.
pub fn resolve_calls(mut program: BcProgram) -> BcProgram {
    let index_of: HashMap<String, usize> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();

    for func in &mut program.functions {
        for instr in &mut func.code {
            if let Instr::CallBuiltin(name, argc) = instr {
                if let Some(fname) = name.strip_prefix("__fn__") {
                    if let Some(&idx) = index_of.get(fname) {
                        *instr = Instr::Call(idx, *argc);
                    }
                }
            }
        }
    }
    program
}
