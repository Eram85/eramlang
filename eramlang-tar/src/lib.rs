//! EramLang compiler, as a library: exposes every pipeline stage so it can
//! be driven programmatically (used by the integration tests and by the
//! `eram` binary itself).

pub mod ast;
pub mod bytecode;
pub mod compiler;
pub mod debugger;
pub mod driver;
pub mod errors;
pub mod interpreter;
pub mod ir;
pub mod lexer;
pub mod optimizer;
pub mod parser;
pub mod repl;
pub mod semantic;
pub mod stdlib;
pub mod token;
pub mod types;
pub mod value;
pub mod vm;
