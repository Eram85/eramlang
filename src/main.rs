use clap::{Parser as ClapParser, Subcommand};
use colored::*;
use eram::{debugger, driver, repl, vm};
use std::fs;
use std::process::ExitCode;

#[derive(ClapParser)]
#[command(
    name = "eram",
    version = "0.1.0",
    about = "EramLang compiler and virtual machine"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Type-check and compile a .er file without running it
    Build { file: String },
    /// Compile and run a .er file
    Run { file: String },
    /// Type-check a .er file and report any errors
    Check { file: String },
    /// Start the interactive REPL
    Repl,
    /// Print the parsed AST for a .er file
    DumpAst { file: String },
    /// Print the intermediate representation for a .er file
    DumpIr { file: String },
    /// Print the compiled bytecode for a .er file
    DumpBytecode { file: String },
    /// Step through a .er file with the interactive debugger
    Debug { file: String },
    /// Print version information
    Version,
}

fn read_file(path: &str) -> Result<String, ExitCode> {
    fs::read_to_string(path).map_err(|e| {
        eprintln!("{}: could not read `{}`: {}", "error".red().bold(), path, e);
        ExitCode::FAILURE
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let command = match cli.command {
        Some(c) => c,
        None => {
            println!(
                "EramLang 0.1.0\nRun `eram --help` for usage, or `eram repl` to start the REPL."
            );
            return ExitCode::SUCCESS;
        }
    };

    match command {
        Command::Version => {
            println!("EramLang 0.1.0\nRust Compiler Backend");
            ExitCode::SUCCESS
        }
        Command::Repl => {
            repl::run_repl();
            ExitCode::SUCCESS
        }
        Command::Check { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::compile(&source, &file) {
                Ok(_) => {
                    println!("{}", "check passed: no errors found".green());
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
        Command::Build { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::compile(&source, &file) {
                Ok(out) => {
                    println!(
                        "{}",
                        format!(
                            "compiled `{}` successfully ({} function(s))",
                            file,
                            out.program.functions.len()
                        )
                        .green()
                    );
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
        Command::Run { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::compile(&source, &file) {
                Ok(out) => {
                    let entry = match out.program.main_index {
                        Some(i) => i,
                        None => {
                            eprintln!("{}: no entry point found", "error".red().bold());
                            return ExitCode::FAILURE;
                        }
                    };
                    let mut vm = vm::Vm::new(&out.program);
                    match vm.run_function(entry, Vec::new()) {
                        Ok(_) => ExitCode::SUCCESS,
                        Err(e) => {
                            eprintln!("{}", e);
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
        Command::Debug { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::compile(&source, &file) {
                Ok(out) => {
                    let entry = match out.program.main_index {
                        Some(i) => i,
                        None => {
                            eprintln!("{}: no entry point found", "error".red().bold());
                            return ExitCode::FAILURE;
                        }
                    };
                    debugger::run_debugger(&out.program, entry);
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
        Command::DumpAst { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::parse_only(&source, &file) {
                Ok(ast) => {
                    println!("{:#?}", ast);
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
        Command::DumpIr { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::compile(&source, &file) {
                Ok(out) => {
                    println!("{}", "-- top level --".bold());
                    for op in &out.ir.top_level {
                        println!("  {:?}", op);
                    }
                    for f in &out.ir.functions {
                        println!("\n{}", format!("-- fn {} --", f.name).bold());
                        for op in &f.body {
                            println!("  {:?}", op);
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
        Command::DumpBytecode { file } => {
            let source = match read_file(&file) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match driver::compile(&source, &file) {
                Ok(out) => {
                    for func in &out.program.functions {
                        println!(
                            "{}",
                            format!(
                                "-- fn {} (arity {}, locals {}) --",
                                func.name, func.arity, func.num_locals
                            )
                            .bold()
                        );
                        for (i, instr) in func.code.iter().enumerate() {
                            println!("  {:>4}  {:?}", i, instr);
                        }
                        println!();
                    }
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprint!("{}", msg);
                    ExitCode::FAILURE
                }
            }
        }
    }
}
