//! The custom bytecode instruction set executed by the virtual machine.
//! A `Program` is a flat table of functions; each `Function` owns its own
//! constant pool and instruction stream. Every instruction is tagged with
//! the source line that generated it, so the VM can report runtime errors
//! with accurate positions.

use crate::value::Value;
use std::fmt;

#[derive(Debug, Clone)]
pub enum Instr {
    // Stack manipulation
    Const(usize), // push constants[idx]
    ConstVoid,
    Pop,
    Dup,

    // Arithmetic / logic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,

    // Variables
    LoadLocal(usize),
    StoreLocal(usize),
    LoadGlobal(usize),
    StoreGlobal(usize),

    // Control flow
    Jump(isize),
    JumpIfFalse(isize),
    JumpIfTrue(isize),
    Call(usize, usize),         // function index, arg count
    CallBuiltin(String, usize), // builtin name, arg count
    Return,
    ReturnVoid,

    // Aggregates
    MakeArray(usize),
    IndexGet,
    IndexSet,
    MakeStruct(String, Vec<String>), // struct name, field names in literal order
    GetField(String),
    SetField(String),
    MakeEnum(String, String),

    // Misc
    Print(bool), // true = println (newline), false = print (no newline)
    Halt,
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub arity: usize,
    pub num_locals: usize,
    pub constants: Vec<Value>,
    pub code: Vec<Instr>,
    pub lines: Vec<usize>,        // parallel to `code`
    pub local_names: Vec<String>, // parallel to local slot indices, for debugging/dumps
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub functions: Vec<Function>,
    pub main_index: Option<usize>,
    pub num_globals: usize,
    pub global_names: Vec<String>,
}

impl Program {
    pub fn find_function(&self, name: &str) -> Option<usize> {
        self.functions.iter().position(|f| f.name == name)
    }
}
