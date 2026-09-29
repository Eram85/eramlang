//! Ties every compiler phase together and is the single place that knows
//! the full pipeline order. Used by the CLI, the REPL, and the tests.

use crate::ast::Program;
use crate::bytecode::Program as BcProgram;
use crate::compiler::{resolve_calls, CodeGen, IrGen};
use crate::errors::Diagnostic;
use crate::ir::IrProgram;
use crate::lexer::Lexer;
use crate::optimizer::optimize_program;
use crate::parser::Parser;
use crate::semantic::{Analyzer, ProgramInfo};

pub struct CompileOutput {
    pub program: BcProgram,
    pub info: ProgramInfo,
    pub ir: IrProgram,
}

/// Runs lexing + parsing only, returning the AST or a rendered error.
pub fn parse_only(source: &str, file: &str) -> Result<Program, String> {
    let tokens = Lexer::new(source, file)
        .tokenize()
        .map_err(|d| d.render(source))?;
    Parser::new(tokens, file)
        .parse_program()
        .map_err(|d| d.render(source))
}

fn render_bag(diags: &crate::errors::DiagnosticBag, source: &str) -> String {
    diags.render_all(source)
}

/// Full pipeline: lex -> parse -> semantic analysis/type check -> IR ->
/// optimize -> bytecode. Returns a human-readable, rendered error string on
/// failure (already formatted for terminal output).
pub fn compile(source: &str, file: &str) -> Result<CompileOutput, String> {
    let ast = parse_only(source, file)?;

    let info = Analyzer::new(file)
        .analyze(&ast)
        .map_err(|bag| render_bag(&bag, source))?;

    let ir = IrGen::new(&info).gen_program(&ast);
    let ir = optimize_program(ir);

    let bc = CodeGen::lower(ir.clone());
    let bc = resolve_calls(bc);

    Ok(CompileOutput {
        program: bc,
        info,
        ir,
    })
}

pub fn render_single(d: &Diagnostic, source: &str) -> String {
    d.render(source)
}
