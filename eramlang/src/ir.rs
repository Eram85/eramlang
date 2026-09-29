//! A small intermediate representation sitting between the AST and the
//! final bytecode. Unlike bytecode, IR addresses locals by *name* and jumps
//! by symbolic *label* rather than resolved slot index / offset, which
//! keeps it independent of both the AST's tree shape and the VM's linear
//! layout. The bytecode compiler ("codegen") lowers IR into `bytecode::Instr`
//! by resolving labels to offsets and names to slots. Optimizations
//! (constant folding, dead-code elimination, constant propagation) run on
//! this representation.

use crate::value::Value;

#[derive(Debug, Clone)]
pub enum IrOp {
    ConstInt(i64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstStr(String),
    ConstChar(char),
    ConstVoid,
    Pop,

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

    LoadLocal(String),
    StoreLocal(String),
    LoadGlobal(String),
    StoreGlobal(String),

    Label(String),
    Jump(String),
    JumpIfFalse(String),
    JumpIfTrue(String),

    Call(String, usize),
    CallBuiltin(String, usize),
    Return,
    ReturnVoid,

    MakeArray(usize),
    IndexGet,
    IndexSet,
    MakeStruct(String, Vec<String>),
    GetField(String),
    SetField(String),
    MakeEnum(String, String),

    Print(bool),
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<IrOp>,
}

#[derive(Debug, Clone, Default)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
    pub top_level: Vec<IrOp>,
    pub has_main: bool,
}

/// Turns a folded constant IR op back into a runtime `Value`, used by the
/// optimizer when it needs to combine two constants at compile time.
pub fn ir_const_value(op: &IrOp) -> Option<Value> {
    match op {
        IrOp::ConstInt(i) => Some(Value::Int(*i)),
        IrOp::ConstFloat(f) => Some(Value::Float(*f)),
        IrOp::ConstBool(b) => Some(Value::Bool(*b)),
        _ => None,
    }
}

pub fn value_to_const_op(v: &Value) -> Option<IrOp> {
    match v {
        Value::Int(i) => Some(IrOp::ConstInt(*i)),
        Value::Float(f) => Some(IrOp::ConstFloat(*f)),
        Value::Bool(b) => Some(IrOp::ConstBool(*b)),
        _ => None,
    }
}
