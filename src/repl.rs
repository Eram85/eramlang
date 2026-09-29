//! Interactive read-eval-print loop. See `interpreter` for why the REPL
//! uses a tree-walking evaluator instead of the bytecode VM.

use crate::ast::{Item, Statement};
use crate::interpreter::ReplSession;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::value::Value;
use colored::*;
use std::io::{self, BufRead, Write};

fn parse_repl_item(src: &str) -> Result<Item, String> {
    let tokens = Lexer::new(src, "<repl>")
        .tokenize()
        .map_err(|d| d.render(src))?;
    let mut p = Parser::new(tokens, "<repl>");
    let item = p.parse_item().map_err(|d| d.render(src))?;
    if !p.is_at_end() {
        return Err("unexpected trailing input".to_string());
    }
    Ok(item)
}

fn try_parse(trimmed: &str) -> Result<Item, String> {
    if let Ok(item) = parse_repl_item(trimmed) {
        return Ok(item);
    }
    let with_semi = format!("{};", trimmed.trim_end_matches(';'));
    parse_repl_item(&with_semi)
}

pub fn run_repl() {
    println!("{}", "EramLang 0.1.0".bold());
    println!("Type EramLang statements or expressions. Type 'exit' to quit.\n");

    let mut session = ReplSession::new();
    let mut pending = String::new();
    let stdin = io::stdin();

    loop {
        if pending.is_empty() {
            print!(">>> ");
        } else {
            print!("... ");
        }
        let _ = io::stdout().flush();

        let mut line = String::new();
        let bytes_read = stdin.lock().read_line(&mut line).unwrap_or(0);
        if bytes_read == 0 {
            println!();
            break; // EOF (Ctrl-D)
        }

        let trimmed_line = line.trim();
        if pending.is_empty() && (trimmed_line == "exit" || trimmed_line == "quit") {
            break;
        }
        if pending.is_empty() && trimmed_line.is_empty() {
            continue;
        }

        pending.push_str(&line);

        let open = pending.matches('{').count();
        let close = pending.matches('}').count();
        if open > close {
            continue; // multiline block in progress
        }

        let chunk = std::mem::take(&mut pending);
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }

        match try_parse(trimmed) {
            Ok(Item::Function(f)) => {
                session.register_item(&Item::Function(f));
            }
            Ok(Item::Struct(s)) => {
                session.register_item(&Item::Struct(s));
            }
            Ok(Item::Enum(e)) => {
                session.register_item(&Item::Enum(e));
            }
            Ok(Item::Statement(stmt)) => {
                let is_bare_expr = matches!(stmt, Statement::ExprStmt(_));
                match session.eval_top_statement(&stmt) {
                    Ok(v) => {
                        if is_bare_expr && !matches!(v, Value::Void) {
                            println!("{}", v);
                        }
                    }
                    Err(e) => println!("{}", format!("Runtime Error: {}", e).red()),
                }
            }
            Err(e) => println!("{}", e),
        }
    }
}
