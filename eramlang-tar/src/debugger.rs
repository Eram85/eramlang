//! A basic debugger that single-steps the VM. Breakpoints are set on
//! *function entry* rather than source line, since bytecode is generated
//! per-function; this is a simplification documented in the README.

use crate::bytecode::Program;
use crate::value::Value;
use crate::vm::Vm;
use colored::*;
use std::collections::HashSet;
use std::io::{self, Write};

pub fn run_debugger(program: &Program, entry: usize) {
    let mut vm = Vm::new(program);
    vm.start(entry, Vec::new());

    let mut breakpoints: HashSet<String> = HashSet::new();
    let mut running_free = false; // true after `continue`, until a breakpoint or step command

    println!("{}", "EramLang Debugger".bold());
    println!("Commands: break <fn>, continue, step, next, print <name>, backtrace, locals, quit\n");

    loop {
        if vm.is_finished() {
            println!("{}", "Program finished.".green());
            break;
        }

        if running_free {
            // Run until a breakpoint (function entry) or program end.
            let hit = loop {
                let fname = vm.current_function_name().unwrap_or("").to_string();
                let at_entry = vm.current_ip() == Some(0);
                if at_entry && breakpoints.contains(&fname) {
                    break Some(fname);
                }
                match vm.step_one() {
                    Ok(Some(_)) => break None, // finished
                    Ok(None) => continue,
                    Err(e) => {
                        println!("{}", e.to_string().red());
                        return;
                    }
                }
            };
            running_free = false;
            if let Some(fname) = hit {
                println!(
                    "{}",
                    format!("Breakpoint hit: entering `{}`", fname).yellow()
                );
            } else if vm.is_finished() {
                println!("{}", "Program finished.".green());
                break;
            }
        }

        print_location(&vm);
        print!("(eram-debug) ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim();
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("break") => {
                if let Some(name) = parts.next() {
                    breakpoints.insert(name.to_string());
                    println!("Breakpoint set at function `{}`", name);
                } else {
                    println!("usage: break <function_name>");
                }
            }
            Some("continue") | Some("c") => {
                running_free = true;
            }
            Some("step") | Some("s") | Some("next") | Some("n") => match vm.step_one() {
                Ok(Some(v)) => {
                    println!("Program finished with value: {}", v);
                    break;
                }
                Ok(None) => {}
                Err(e) => println!("{}", e.to_string().red()),
            },
            Some("print") | Some("p") => {
                if let Some(name) = parts.next() {
                    let locals = vm.current_locals();
                    match locals.iter().find(|(n, _)| display_name(n) == name) {
                        Some((_, v)) => println!("{} = {}", name, v),
                        None => println!("no such local `{}`", name),
                    }
                } else {
                    println!("usage: print <name>");
                }
            }
            Some("backtrace") | Some("bt") => {
                for (i, f) in vm.backtrace().iter().enumerate() {
                    println!("#{}  {}", i, f);
                }
            }
            Some("locals") => {
                let locals = vm.current_locals();
                if locals.is_empty() {
                    println!("(no locals)");
                }
                for (n, v) in locals {
                    if n.starts_with("__t") {
                        continue; // hide compiler-generated temporaries
                    }
                    println!("{} = {}", display_name(&n), v);
                }
            }
            Some("quit") | Some("q") => break,
            Some(other) => println!("unknown command `{}`", other),
            None => {}
        }
    }
}

/// Strips the compiler's internal uniqueness suffix (`n$1` -> `n`) so the
/// debugger shows and matches the name the user actually wrote.
fn display_name(mangled: &str) -> &str {
    match mangled.rfind('$') {
        Some(idx) => &mangled[..idx],
        None => mangled,
    }
}

fn print_location(vm: &Vm) {
    let fname = vm.current_function_name().unwrap_or("<none>");
    let ip = vm.current_ip().unwrap_or(0);
    println!("{}", format!("-- {} (ip {}) --", fname, ip).cyan());
}

#[allow(dead_code)]
fn unused(_: Value) {}
