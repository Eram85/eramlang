//! EramLang's standard library. Kept deliberately separate from the
//! compiler: this module only knows about `Value`s and I/O, never about
//! the AST or bytecode.

use crate::ast::TypeAnnotation as T;
use crate::value::{StructValue, Value};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

/// Names of every builtin, for the semantic analyzer's function table.
/// Types are advisory (`Unknown` accepts anything) since several builtins
/// are generic over element type.
pub fn builtin_signatures() -> Vec<(&'static str, Vec<T>, T)> {
    vec![
        ("print", vec![T::Unknown], T::Void),
        ("println", vec![T::Unknown], T::Void),
        ("input", vec![], T::Str),
        ("len", vec![T::Unknown], T::Int),
        ("push", vec![T::Unknown, T::Unknown], T::Void),
        ("pop", vec![T::Unknown], T::Unknown),
        ("sqrt", vec![T::Unknown], T::Float),
        ("abs", vec![T::Unknown], T::Unknown),
        ("min", vec![T::Unknown, T::Unknown], T::Unknown),
        ("max", vec![T::Unknown, T::Unknown], T::Unknown),
    ]
}

/// Builtins whose real arity differs from the table above (e.g. `print`
/// accepts any number of arguments in practice). The semantic analyzer
/// skips strict arity checking for these.
pub fn is_variadic(name: &str) -> bool {
    matches!(name, "print" | "println" | "min" | "max")
}

pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "print" | "println" | "input" | "len" | "push" | "pop" | "sqrt" | "abs" | "min" | "max"
    )
}

#[derive(Debug)]
pub enum RuntimeErr {
    Message(String),
}

/// Executes a builtin call. Returns the resulting value or a runtime error
/// message (never panics).
pub fn call_builtin(name: &str, mut args: Vec<Value>) -> Result<Value, RuntimeErr> {
    match name {
        "print" => {
            for a in &args {
                print!("{}", a);
            }
            let _ = std::io::stdout().flush();
            Ok(Value::Void)
        }
        "println" => {
            let mut s = String::new();
            for a in &args {
                s.push_str(&a.to_string());
            }
            println!("{}", s);
            Ok(Value::Void)
        }
        "input" => {
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .map_err(|e| RuntimeErr::Message(format!("failed to read input: {}", e)))?;
            Ok(Value::Str(Rc::new(
                buf.trim_end_matches(['\n', '\r']).to_string(),
            )))
        }
        "len" => match args.pop() {
            Some(Value::Array(a)) => Ok(Value::Int(a.borrow().len() as i64)),
            Some(Value::Str(s)) => Ok(Value::Int(s.chars().count() as i64)),
            Some(other) => Err(RuntimeErr::Message(format!(
                "`len` expects an array or string, found `{}`",
                other.type_name()
            ))),
            None => Err(RuntimeErr::Message("`len` expects 1 argument".into())),
        },
        "push" => {
            if args.len() != 2 {
                return Err(RuntimeErr::Message("`push` expects 2 arguments".into()));
            }
            let value = args.pop().unwrap();
            let arr = args.pop().unwrap();
            match arr {
                Value::Array(a) => {
                    a.borrow_mut().push(value);
                    Ok(Value::Void)
                }
                other => Err(RuntimeErr::Message(format!(
                    "`push` expects an array, found `{}`",
                    other.type_name()
                ))),
            }
        }
        "pop" => match args.pop() {
            Some(Value::Array(a)) => {
                let mut v = a.borrow_mut();
                v.pop()
                    .ok_or_else(|| RuntimeErr::Message("`pop` on empty array".into()))
            }
            Some(other) => Err(RuntimeErr::Message(format!(
                "`pop` expects an array, found `{}`",
                other.type_name()
            ))),
            None => Err(RuntimeErr::Message("`pop` expects 1 argument".into())),
        },
        "sqrt" => match args.pop() {
            Some(Value::Int(i)) => Ok(Value::Float((i as f64).sqrt())),
            Some(Value::Float(f)) => Ok(Value::Float(f.sqrt())),
            Some(other) => Err(RuntimeErr::Message(format!(
                "`sqrt` expects a number, found `{}`",
                other.type_name()
            ))),
            None => Err(RuntimeErr::Message("`sqrt` expects 1 argument".into())),
        },
        "abs" => match args.pop() {
            Some(Value::Int(i)) => Ok(Value::Int(i.abs())),
            Some(Value::Float(f)) => Ok(Value::Float(f.abs())),
            Some(other) => Err(RuntimeErr::Message(format!(
                "`abs` expects a number, found `{}`",
                other.type_name()
            ))),
            None => Err(RuntimeErr::Message("`abs` expects 1 argument".into())),
        },
        "min" | "max" => {
            if args.is_empty() {
                return Err(RuntimeErr::Message(format!(
                    "`{}` expects at least 1 argument",
                    name
                )));
            }
            let mut best = args.remove(0);
            for a in args {
                let take = match (&a, &best) {
                    (Value::Int(x), Value::Int(y)) => {
                        if name == "min" {
                            x < y
                        } else {
                            x > y
                        }
                    }
                    (Value::Float(x), Value::Float(y)) => {
                        if name == "min" {
                            x < y
                        } else {
                            x > y
                        }
                    }
                    (Value::Int(x), Value::Float(y)) => {
                        let xf = *x as f64;
                        if name == "min" {
                            xf < *y
                        } else {
                            xf > *y
                        }
                    }
                    (Value::Float(x), Value::Int(y)) => {
                        let yf = *y as f64;
                        if name == "min" {
                            *x < yf
                        } else {
                            *x > yf
                        }
                    }
                    _ => {
                        return Err(RuntimeErr::Message(format!(
                            "`{}` expects numeric arguments",
                            name
                        )))
                    }
                };
                if take {
                    best = a;
                }
            }
            Ok(best)
        }
        other => Err(RuntimeErr::Message(format!(
            "unknown builtin function `{}`",
            other
        ))),
    }
}

#[allow(dead_code)]
pub fn new_struct(name: &str, fields: Vec<(String, Value)>) -> Value {
    Value::Struct(Rc::new(StructValue {
        name: name.to_string(),
        fields: RefCell::new(fields),
    }))
}
