//! Modular optimization passes over the IR. Each pass is a pure function
//! `Vec<IrOp> -> Vec<IrOp>` so passes can be composed, reordered, disabled,
//! or benchmarked independently.

use crate::ir::{ir_const_value, value_to_const_op, IrOp, IrProgram};
use crate::value::Value;

pub fn optimize_program(mut prog: IrProgram) -> IrProgram {
    prog.top_level = optimize_block(prog.top_level);
    for f in &mut prog.functions {
        f.body = optimize_block(std::mem::take(&mut f.body));
    }
    prog
}

fn optimize_block(ops: Vec<IrOp>) -> Vec<IrOp> {
    let ops = constant_fold(ops);
    let ops = constant_propagate(ops);
    dead_code_eliminate(ops)
}

/// Peephole constant folding: `Const a, Const b, <binop>` collapses into a
/// single `Const` whenever both operands are compile-time constants.
fn constant_fold(ops: Vec<IrOp>) -> Vec<IrOp> {
    let mut out: Vec<IrOp> = Vec::with_capacity(ops.len());
    for op in ops {
        let folded = if let IrOp::Add | IrOp::Sub | IrOp::Mul | IrOp::Div | IrOp::Mod = op {
            fold_binary(&out, &op)
        } else if let IrOp::Neg = op {
            fold_unary_neg(&out)
        } else {
            None
        };

        if let Some((new_len, new_op)) = folded {
            out.truncate(new_len);
            out.push(new_op);
        } else {
            out.push(op);
        }
    }
    out
}

fn fold_binary(out: &[IrOp], op: &IrOp) -> Option<(usize, IrOp)> {
    let len = out.len();
    if len < 2 {
        return None;
    }
    let b = ir_const_value(&out[len - 1])?;
    let a = ir_const_value(&out[len - 2])?;
    let result = apply_arith(op, &a, &b)?;
    Some((len - 2, value_to_const_op(&result)?))
}

fn fold_unary_neg(out: &[IrOp]) -> Option<(usize, IrOp)> {
    let len = out.len();
    if len < 1 {
        return None;
    }
    let a = ir_const_value(&out[len - 1])?;
    let result = match a {
        Value::Int(i) => Value::Int(-i),
        Value::Float(f) => Value::Float(-f),
        _ => return None,
    };
    Some((len - 1, value_to_const_op(&result)?))
}

fn apply_arith(op: &IrOp, a: &Value, b: &Value) -> Option<Value> {
    use Value::*;
    Some(match (a, b) {
        (Int(x), Int(y)) => match op {
            IrOp::Add => Int(x + y),
            IrOp::Sub => Int(x - y),
            IrOp::Mul => Int(x * y),
            IrOp::Div => {
                if *y == 0 {
                    return None;
                }
                Int(x / y)
            }
            IrOp::Mod => {
                if *y == 0 {
                    return None;
                }
                Int(x % y)
            }
            _ => return None,
        },
        (Float(x), Float(y)) => match op {
            IrOp::Add => Float(x + y),
            IrOp::Sub => Float(x - y),
            IrOp::Mul => Float(x * y),
            IrOp::Div => Float(x / y),
            IrOp::Mod => Float(x % y),
            _ => return None,
        },
        _ => return None,
    })
}

/// Very small local constant-propagation pass: when a local is assigned a
/// literal constant and never reassigned before its next use within the
/// same straight-line block, later `LoadLocal` for that name are replaced
/// with the constant directly. Conservative: stops tracking a variable the
/// moment it sees a label, call, or loop-like control edge, since it cannot
/// prove the value still holds there.
fn constant_propagate(ops: Vec<IrOp>) -> Vec<IrOp> {
    use std::collections::HashMap;
    let mut known: HashMap<String, IrOp> = HashMap::new();
    let mut out = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        match &ops[i] {
            IrOp::StoreLocal(name) => {
                // The value just pushed is the top of `out` if it's a const op.
                if let Some(last) = out.last() {
                    if ir_const_value(last).is_some() {
                        known.insert(name.clone(), last.clone());
                    } else {
                        known.remove(name);
                    }
                }
                out.push(ops[i].clone());
            }
            IrOp::LoadLocal(name) => {
                if let Some(c) = known.get(name) {
                    out.push(c.clone());
                } else {
                    out.push(ops[i].clone());
                }
            }
            IrOp::Label(_)
            | IrOp::Call(..)
            | IrOp::CallBuiltin(..)
            | IrOp::Jump(_)
            | IrOp::JumpIfFalse(_) => {
                known.clear();
                out.push(ops[i].clone());
            }
            other => out.push(other.clone()),
        }
        i += 1;
    }
    out
}

/// Removes instructions that can provably never execute: anything between
/// an unconditional `Jump`/`Return` and the next `Label` in the same block
/// is unreachable.
fn dead_code_eliminate(ops: Vec<IrOp>) -> Vec<IrOp> {
    let mut out = Vec::with_capacity(ops.len());
    let mut unreachable = false;
    for op in ops {
        match &op {
            IrOp::Label(_) => {
                unreachable = false;
                out.push(op);
            }
            _ => {
                if unreachable {
                    continue; // drop dead instruction
                }
                let terminates = matches!(op, IrOp::Jump(_) | IrOp::Return | IrOp::ReturnVoid);
                out.push(op);
                if terminates {
                    unreachable = true;
                }
            }
        }
    }
    out
}
