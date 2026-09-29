//! Static type checking support. `TypeAnnotation` itself lives in `ast`;
//! this module holds the compatibility and inference rules the semantic
//! analyzer uses to type-check EramLang programs.

use crate::ast::TypeAnnotation as T;

/// Are two types compatible for assignment / comparison purposes?
/// `Unknown` is compatible with anything (used before inference fills it in).
pub fn compatible(a: &T, b: &T) -> bool {
    if *a == T::Unknown || *b == T::Unknown {
        return true;
    }
    match (a, b) {
        (T::Array(x), T::Array(y)) => compatible(x, y),
        _ => a == b,
    }
}

/// Result type of a binary arithmetic operation, if the operand types are
/// valid together; `None` means the operator is not defined for these types.
pub fn arith_result(op_is_add: bool, a: &T, b: &T) -> Option<T> {
    match (a, b) {
        (T::Int, T::Int) => Some(T::Int),
        (T::Float, T::Float) => Some(T::Float),
        (T::Int, T::Float) | (T::Float, T::Int) => Some(T::Float),
        (T::Str, T::Str) if op_is_add => Some(T::Str),
        (T::Unknown, other) | (other, T::Unknown) => Some(other.clone()),
        _ => None,
    }
}

pub fn is_numeric(t: &T) -> bool {
    matches!(t, T::Int | T::Float | T::Unknown)
}
