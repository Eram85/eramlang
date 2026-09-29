//! Runtime values manipulated by the virtual machine.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<String>),
    Char(char),
    Array(Rc<RefCell<Vec<Value>>>),
    Struct(Rc<StructValue>),
    Enum(Rc<String>, Rc<String>), // enum name, variant name
    Void,
}

#[derive(Debug)]
pub struct StructValue {
    pub name: String,
    pub fields: RefCell<Vec<(String, Value)>>,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Char(_) => "char",
            Value::Array(_) => "array",
            Value::Struct(_) => "struct",
            Value::Enum(_, _) => "enum",
            Value::Void => "void",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            _ => true,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(v) => write!(f, "{}", v),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Str(s) => write!(f, "{}", s),
            Value::Char(c) => write!(f, "{}", c),
            Value::Array(a) => {
                let items = a.borrow();
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Struct(s) => {
                write!(f, "{} {{ ", s.name)?;
                let fields = s.fields.borrow();
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::Enum(_enum_name, variant) => write!(f, "{}", variant),
            Value::Void => write!(f, "void"),
        }
    }
}
