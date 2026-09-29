//! A stack-based virtual machine. Owns an operand stack, a call stack of
//! frames (each with its own local-variable slots and instruction
//! pointer), and executes `bytecode::Program`s. Runtime errors are
//! returned as `Result`s -- the VM never panics on malformed *language*
//! programs (a `RuntimeError` is always produced instead), matching the
//! "never crash with an unexplained Rust panic" requirement.

use crate::bytecode::{Function, Instr, Program};
use crate::stdlib;
use crate::value::{StructValue, Value};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

const MAX_CALL_DEPTH: usize = 2000;

#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
    pub function: String,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Runtime Error:\n{}\n  (in function `{}`)",
            self.message, self.function
        )
    }
}

struct Frame {
    function_index: usize,
    ip: usize,
    locals: Vec<Value>,
    base_stack_len: usize,
}

pub struct Vm<'p> {
    program: &'p Program,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    pub instructions_executed: u64,
}

impl<'p> Vm<'p> {
    pub fn new(program: &'p Program) -> Self {
        Vm {
            program,
            stack: Vec::new(),
            frames: Vec::new(),
            instructions_executed: 0,
        }
    }

    pub fn run_function(&mut self, index: usize, args: Vec<Value>) -> Result<Value, RuntimeError> {
        self.start(index, args);
        self.execute()
    }

    /// Pushes the initial call frame without running anything; used by the
    /// debugger to drive execution one instruction at a time.
    pub fn start(&mut self, index: usize, args: Vec<Value>) {
        let func = &self.program.functions[index];
        let mut locals = vec![Value::Void; func.num_locals.max(args.len())];
        for (i, a) in args.into_iter().enumerate() {
            locals[i] = a;
        }
        self.frames.push(Frame {
            function_index: index,
            ip: 0,
            locals,
            base_stack_len: self.stack.len(),
        });
    }

    pub fn is_finished(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn backtrace(&self) -> Vec<String> {
        self.frames
            .iter()
            .rev()
            .map(|f| self.program.functions[f.function_index].name.clone())
            .collect()
    }

    pub fn current_function_name(&self) -> Option<&str> {
        self.frames
            .last()
            .map(|f| self.program.functions[f.function_index].name.as_str())
    }

    pub fn current_ip(&self) -> Option<usize> {
        self.frames.last().map(|f| f.ip)
    }

    /// Local variable names/values in the current frame, for `locals` and
    /// `print <name>` in the debugger.
    pub fn current_locals(&self) -> Vec<(String, Value)> {
        match self.frames.last() {
            None => Vec::new(),
            Some(f) => {
                let func = &self.program.functions[f.function_index];
                func.local_names
                    .iter()
                    .zip(f.locals.iter())
                    .map(|(n, v)| (n.clone(), v.clone()))
                    .collect()
            }
        }
    }

    /// Runs exactly one bytecode instruction. Returns `Ok(Some(value))`
    /// once the whole call stack has unwound (program finished with this
    /// value), or `Ok(None)` if execution should continue.
    pub fn step_one(&mut self) -> Result<Option<Value>, RuntimeError> {
        if self.frames.is_empty() {
            return Ok(Some(Value::Void));
        }
        let (ip, func_idx) = {
            let f = self.frames.last().unwrap();
            (f.ip, f.function_index)
        };
        let func = &self.program.functions[func_idx];
        if ip >= func.code.len() {
            let finished = self.frames.pop().unwrap();
            self.stack.truncate(finished.base_stack_len);
            if self.frames.is_empty() {
                return Ok(Some(Value::Void));
            }
            self.stack.push(Value::Void);
            return Ok(None);
        }
        let instr = func.code[ip].clone();
        self.frames.last_mut().unwrap().ip += 1;
        self.instructions_executed += 1;

        match self.step(instr, func)? {
            StepResult::Continue => Ok(None),
            StepResult::Returned(v) => {
                let finished = self.frames.pop().unwrap();
                self.stack.truncate(finished.base_stack_len);
                if self.frames.is_empty() {
                    return Ok(Some(v));
                }
                self.stack.push(v);
                Ok(None)
            }
        }
    }

    fn rt_err(&self, msg: impl Into<String>) -> RuntimeError {
        let fname = self
            .frames
            .last()
            .map(|f| self.program.functions[f.function_index].name.clone())
            .unwrap_or_else(|| "<top>".to_string());
        RuntimeError {
            message: msg.into(),
            function: fname,
        }
    }

    fn pop(&mut self) -> Result<Value, RuntimeError> {
        self.stack
            .pop()
            .ok_or_else(|| self.rt_err("operand stack underflow"))
    }

    #[allow(dead_code)]
    fn current_func(&self) -> &Function {
        let idx = self.frames.last().unwrap().function_index;
        &self.program.functions[idx]
    }

    fn execute(&mut self) -> Result<Value, RuntimeError> {
        loop {
            if let Some(v) = self.step_one()? {
                return Ok(v);
            }
        }
    }

    fn step(
        &mut self,
        instr: Instr,
        func_for_consts: &Function,
    ) -> Result<StepResult, RuntimeError> {
        match instr {
            Instr::Const(idx) => {
                self.stack.push(func_for_consts.constants[idx].clone());
            }
            Instr::ConstVoid => self.stack.push(Value::Void),
            Instr::Pop => {
                self.pop()?;
            }
            Instr::Dup => {
                let v = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| self.rt_err("stack underflow on dup"))?;
                self.stack.push(v);
            }
            Instr::Add => self.binary_arith(|a, b| a.checked_add(b), |a, b| a + b, "+")?,
            Instr::Sub => self.binary_arith(|a, b| a.checked_sub(b), |a, b| a - b, "-")?,
            Instr::Mul => self.binary_arith(|a, b| a.checked_mul(b), |a, b| a * b, "*")?,
            Instr::Div => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(_), Value::Int(0)) => return Err(self.rt_err("division by zero")),
                    (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                    (Value::Int(x), Value::Float(y)) => Value::Float(*x as f64 / y),
                    (Value::Float(x), Value::Int(y)) => {
                        if *y == 0 {
                            return Err(self.rt_err("division by zero"));
                        }
                        Value::Float(x / *y as f64)
                    }
                    _ => {
                        return Err(self.rt_err(format!(
                            "invalid operand types for '/': `{}`, `{}`",
                            a.type_name(),
                            b.type_name()
                        )))
                    }
                };
                self.stack.push(result);
            }
            Instr::Mod => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(_), Value::Int(0)) => {
                        return Err(self.rt_err("division by zero (in modulo)"))
                    }
                    (Value::Int(x), Value::Int(y)) => Value::Int(x % y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x % y),
                    _ => {
                        return Err(self.rt_err(format!(
                            "invalid operand types for '%': `{}`, `{}`",
                            a.type_name(),
                            b.type_name()
                        )))
                    }
                };
                self.stack.push(result);
            }
            Instr::Neg => {
                let a = self.pop()?;
                let r = match a {
                    Value::Int(x) => Value::Int(-x),
                    Value::Float(x) => Value::Float(-x),
                    other => {
                        return Err(self.rt_err(format!("cannot negate `{}`", other.type_name())))
                    }
                };
                self.stack.push(r);
            }
            Instr::Not => {
                let a = self.pop()?;
                self.stack.push(Value::Bool(!a.is_truthy()));
            }
            Instr::Eq => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.stack.push(Value::Bool(values_equal(&a, &b)));
            }
            Instr::Neq => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.stack.push(Value::Bool(!values_equal(&a, &b)));
            }
            Instr::Lt => self.compare(|o| o == std::cmp::Ordering::Less)?,
            Instr::Gt => self.compare(|o| o == std::cmp::Ordering::Greater)?,
            Instr::Le => self.compare(|o| o != std::cmp::Ordering::Greater)?,
            Instr::Ge => self.compare(|o| o != std::cmp::Ordering::Less)?,
            Instr::And => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.stack.push(Value::Bool(a.is_truthy() && b.is_truthy()));
            }
            Instr::Or => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.stack.push(Value::Bool(a.is_truthy() || b.is_truthy()));
            }
            Instr::LoadLocal(idx) => {
                let frame = self.frames.last().unwrap();
                let v = frame
                    .locals
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| self.rt_err("undefined local slot"))?;
                self.stack.push(v);
            }
            Instr::StoreLocal(idx) => {
                let v = self.pop()?;
                let frame = self.frames.last_mut().unwrap();
                if idx >= frame.locals.len() {
                    frame.locals.resize(idx + 1, Value::Void);
                }
                frame.locals[idx] = v;
            }
            Instr::LoadGlobal(_) | Instr::StoreGlobal(_) => {
                return Err(self.rt_err("globals are not used by this compiler"));
            }
            Instr::Jump(target) => {
                self.frames.last_mut().unwrap().ip = target as usize;
            }
            Instr::JumpIfFalse(target) => {
                let v = self.pop()?;
                if !v.is_truthy() {
                    self.frames.last_mut().unwrap().ip = target as usize;
                }
            }
            Instr::JumpIfTrue(target) => {
                let v = self.pop()?;
                if v.is_truthy() {
                    self.frames.last_mut().unwrap().ip = target as usize;
                }
            }
            Instr::Call(func_idx, argc) => {
                if self.frames.len() >= MAX_CALL_DEPTH {
                    return Err(self.rt_err("stack overflow: call depth exceeded"));
                }
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(self.pop()?);
                }
                args.reverse();
                let callee = &self.program.functions[func_idx];
                let mut locals = vec![Value::Void; callee.num_locals.max(args.len())];
                for (i, a) in args.into_iter().enumerate() {
                    locals[i] = a;
                }
                self.frames.push(Frame {
                    function_index: func_idx,
                    ip: 0,
                    locals,
                    base_stack_len: self.stack.len(),
                });
            }
            Instr::CallBuiltin(name, argc) => {
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(self.pop()?);
                }
                args.reverse();
                match stdlib::call_builtin(&name, args) {
                    Ok(v) => self.stack.push(v),
                    Err(stdlib::RuntimeErr::Message(m)) => return Err(self.rt_err(m)),
                }
            }
            Instr::Return => {
                let v = self.pop()?;
                return Ok(StepResult::Returned(v));
            }
            Instr::ReturnVoid => {
                return Ok(StepResult::Returned(Value::Void));
            }
            Instr::MakeArray(n) => {
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.pop()?);
                }
                items.reverse();
                self.stack.push(Value::Array(Rc::new(RefCell::new(items))));
            }
            Instr::IndexGet => {
                let index = self.pop()?;
                let arr = self.pop()?;
                let result = match (&arr, &index) {
                    (Value::Array(a), Value::Int(i)) => {
                        let items = a.borrow();
                        let idx = *i;
                        if idx < 0 || idx as usize >= items.len() {
                            return Err(self.rt_err(format!(
                                "array index out of bounds: index {} for length {}",
                                idx,
                                items.len()
                            )));
                        }
                        items[idx as usize].clone()
                    }
                    (Value::Str(s), Value::Int(i)) => {
                        let chars: Vec<char> = s.chars().collect();
                        let idx = *i;
                        if idx < 0 || idx as usize >= chars.len() {
                            return Err(self.rt_err("string index out of bounds"));
                        }
                        Value::Char(chars[idx as usize])
                    }
                    _ => {
                        return Err(self.rt_err(format!(
                            "cannot index `{}` with `{}`",
                            arr.type_name(),
                            index.type_name()
                        )))
                    }
                };
                self.stack.push(result);
            }
            Instr::IndexSet => {
                let value = self.pop()?;
                let index = self.pop()?;
                let arr = self.pop()?;
                match (&arr, &index) {
                    (Value::Array(a), Value::Int(i)) => {
                        let mut items = a.borrow_mut();
                        let idx = *i;
                        if idx < 0 || idx as usize >= items.len() {
                            return Err(self.rt_err(format!(
                                "array index out of bounds: index {} for length {}",
                                idx,
                                items.len()
                            )));
                        }
                        items[idx as usize] = value;
                    }
                    _ => {
                        return Err(self.rt_err(format!(
                            "cannot assign into `{}` with index `{}`",
                            arr.type_name(),
                            index.type_name()
                        )))
                    }
                }
            }
            Instr::MakeStruct(name, field_names) => {
                let mut values = Vec::with_capacity(field_names.len());
                for _ in 0..field_names.len() {
                    values.push(self.pop()?);
                }
                values.reverse();
                let fields: Vec<(String, Value)> = field_names.into_iter().zip(values).collect();
                self.stack.push(Value::Struct(Rc::new(StructValue {
                    name,
                    fields: RefCell::new(fields),
                })));
            }
            Instr::GetField(field) => {
                let obj = self.pop()?;
                match &obj {
                    Value::Struct(s) => {
                        let fields = s.fields.borrow();
                        match fields.iter().find(|(n, _)| *n == field) {
                            Some((_, v)) => self.stack.push(v.clone()),
                            None => {
                                return Err(self.rt_err(format!(
                                    "struct `{}` has no field `{}`",
                                    s.name, field
                                )))
                            }
                        }
                    }
                    other => {
                        return Err(self.rt_err(format!(
                            "cannot access field `{}` on `{}`",
                            field,
                            other.type_name()
                        )))
                    }
                }
            }
            Instr::SetField(field) => {
                let value = self.pop()?;
                let obj = self.pop()?;
                match &obj {
                    Value::Struct(s) => {
                        let mut fields = s.fields.borrow_mut();
                        match fields.iter_mut().find(|(n, _)| *n == field) {
                            Some(slot) => slot.1 = value,
                            None => {
                                return Err(self.rt_err(format!(
                                    "struct `{}` has no field `{}`",
                                    s.name, field
                                )))
                            }
                        }
                    }
                    other => {
                        return Err(self.rt_err(format!(
                            "cannot set field `{}` on `{}`",
                            field,
                            other.type_name()
                        )))
                    }
                }
            }
            Instr::MakeEnum(enum_name, variant) => {
                self.stack
                    .push(Value::Enum(Rc::new(enum_name), Rc::new(variant)));
            }
            Instr::Print(newline) => {
                let v = self.pop()?;
                if newline {
                    println!("{}", v);
                } else {
                    print!("{}", v);
                }
            }
            Instr::Halt => {
                self.frames.clear();
            }
        }
        Ok(StepResult::Continue)
    }

    fn binary_arith(
        &mut self,
        int_op: impl Fn(i64, i64) -> Option<i64>,
        float_op: impl Fn(f64, f64) -> f64,
        symbol: &str,
    ) -> Result<(), RuntimeError> {
        let b = self.pop()?;
        let a = self.pop()?;
        let result = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => match int_op(*x, *y) {
                Some(v) => Value::Int(v),
                None => {
                    return Err(self.rt_err(format!("integer overflow in '{}' operation", symbol)))
                }
            },
            (Value::Float(x), Value::Float(y)) => Value::Float(float_op(*x, *y)),
            (Value::Int(x), Value::Float(y)) => Value::Float(float_op(*x as f64, *y)),
            (Value::Float(x), Value::Int(y)) => Value::Float(float_op(*x, *y as f64)),
            (Value::Str(x), Value::Str(y)) if symbol == "+" => {
                Value::Str(Rc::new(format!("{}{}", x, y)))
            }
            (Value::Str(x), other) if symbol == "+" => {
                Value::Str(Rc::new(format!("{}{}", x, other)))
            }
            _ => {
                return Err(self.rt_err(format!(
                    "invalid operand types `{}` and `{}` for '{}'",
                    a.type_name(),
                    b.type_name(),
                    symbol
                )))
            }
        };
        self.stack.push(result);
        Ok(())
    }

    fn compare(&mut self, pred: impl Fn(std::cmp::Ordering) -> bool) -> Result<(), RuntimeError> {
        let b = self.pop()?;
        let a = self.pop()?;
        let ord = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
            (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
            (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
            (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
            (Value::Char(x), Value::Char(y)) => x.partial_cmp(y),
            (Value::Str(x), Value::Str(y)) => x.partial_cmp(y),
            _ => {
                return Err(self.rt_err(format!(
                    "cannot compare `{}` with `{}`",
                    a.type_name(),
                    b.type_name()
                )))
            }
        };
        match ord {
            Some(o) => {
                self.stack.push(Value::Bool(pred(o)));
                Ok(())
            }
            None => Err(self.rt_err("comparison produced no ordering (NaN?)")),
        }
    }
}

enum StepResult {
    Continue,
    Returned(Value),
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => *x as f64 == *y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Enum(e1, v1), Value::Enum(e2, v2)) => e1 == e2 && v1 == v2,
        (Value::Void, Value::Void) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::driver::compile;
    use crate::vm::Vm;

    fn run(src: &str) -> Result<crate::value::Value, String> {
        let out = compile(src, "<test>").map_err(|e| e)?;
        let entry = out.program.main_index.expect("entry point");
        let mut vm = Vm::new(&out.program);
        vm.run_function(entry, Vec::new())
            .map_err(|e| e.to_string())
    }

    #[test]
    fn test_vm_arithmetic() {
        // No stdout capture needed: we just check it runs without error and
        // executes at least one instruction.
        let out = compile("fn main() { let x = 2 + 3 * 4; }", "<test>").unwrap();
        let entry = out.program.main_index.unwrap();
        let mut vm = Vm::new(&out.program);
        let result = vm.run_function(entry, Vec::new());
        assert!(result.is_ok());
        assert!(vm.instructions_executed > 0);
    }

    #[test]
    fn test_vm_function_calls() {
        let result =
            run("fn add(a: int, b: int) -> int { return a + b; } fn main() { let x = add(2, 3); }");
        assert!(result.is_ok());
    }

    #[test]
    fn test_vm_division_by_zero() {
        let result = run("fn main() { let x = 1 / 0; }");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("division by zero"));
    }

    #[test]
    fn test_vm_recursive_calls() {
        let result = run(
            "fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); } fn main() { let x = fact(10); }",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_vm_stack_overflow_reports_runtime_error_not_panic() {
        // Infinite recursion should surface as a RuntimeError, never a
        // Rust panic / stack overflow crash.
        let result = run("fn loop_forever(n: int) -> int { return loop_forever(n + 1); } fn main() { let x = loop_forever(0); }");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("stack overflow"));
    }
}
