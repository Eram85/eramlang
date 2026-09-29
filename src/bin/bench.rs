//! Performance benchmarks for each compiler stage, plus VM execution.
//! Run with: `cargo run --release --bin bench`
//!
//! Deliberately dependency-free (no `criterion`) to keep the dependency
//! list minimal, per the project's technology constraints. Timings are
//! wall-clock via `std::time::Instant`, averaged over a few iterations.

use eram::driver::compile;
use eram::lexer::Lexer;
use eram::parser::Parser;
use eram::vm::Vm;
use std::time::Instant;

fn make_loop_program(iterations: u64) -> String {
    format!(
        "fn main() {{\n    let mut i = 0;\n    let mut sum = 0;\n    while i < {} {{\n        sum = sum + i;\n        i = i + 1;\n    }}\n    println(sum);\n}}\n",
        iterations
    )
}

fn make_array_program(iterations: u64) -> String {
    format!(
        "fn main() {{\n    let mut arr = [0];\n    let mut i = 0;\n    while i < {} {{\n        push(arr, i);\n        i = i + 1;\n    }}\n    println(len(arr));\n}}\n",
        iterations
    )
}

fn time_it<F: FnMut()>(mut f: F, iters: u32) -> f64 {
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("EramLang Performance Benchmarks");
    println!("================================\n");

    for &n in &[1_000u64, 10_000, 100_000, 1_000_000] {
        let src = make_loop_program(n);

        let lex_ms = time_it(
            || {
                let _ = Lexer::new(&src, "<bench>").tokenize().unwrap();
            },
            5,
        );

        let tokens = Lexer::new(&src, "<bench>").tokenize().unwrap();
        let parse_ms = time_it(
            || {
                let _ = Parser::new(tokens.clone(), "<bench>")
                    .parse_program()
                    .unwrap();
            },
            5,
        );

        let compile_ms = time_it(
            || {
                let _ = compile(&src, "<bench>").unwrap();
            },
            5,
        );

        let out = compile(&src, "<bench>").unwrap();
        let entry = out.program.main_index.unwrap();
        let mut instrs_executed = 0u64;
        let exec_ms = time_it(
            || {
                let mut vm = Vm::new(&out.program);
                vm.run_function(entry, Vec::new()).unwrap();
                instrs_executed = vm.instructions_executed;
            },
            3,
        );

        println!("Benchmark: while-loop summing {} iterations", n);
        println!("  Lexing:      {:.3} ms", lex_ms);
        println!("  Parsing:     {:.3} ms", parse_ms);
        println!("  Compilation: {:.3} ms", compile_ms);
        println!("  Execution:   {:.3} ms", exec_ms);
        println!("  Instructions executed: {}", instrs_executed);
        println!();
    }

    println!("Benchmark: array push (function calls + growth)");
    for &n in &[1_000u64, 10_000, 100_000] {
        let src = make_array_program(n);
        let out = compile(&src, "<bench>").unwrap();
        let entry = out.program.main_index.unwrap();
        let exec_ms = time_it(
            || {
                let mut vm = Vm::new(&out.program);
                vm.run_function(entry, Vec::new()).unwrap();
            },
            3,
        );
        println!("  n={:>8}  execution: {:.3} ms", n, exec_ms);
    }

    println!("\nBenchmark: recursive function calls (fibonacci)");
    for &n in &[20u64, 25, 27] {
        let src = format!(
            "fn fibonacci(n: int) -> int {{\n    if n <= 1 {{ return n; }}\n    return fibonacci(n - 1) + fibonacci(n - 2);\n}}\nfn main() {{ println(fibonacci({})); }}\n",
            n
        );
        let compile_start = Instant::now();
        let out = compile(&src, "<bench>").unwrap();
        let compile_ms = compile_start.elapsed().as_secs_f64() * 1000.0;
        let entry = out.program.main_index.unwrap();
        let mut instrs = 0u64;
        let exec_ms = time_it(
            || {
                let mut vm = Vm::new(&out.program);
                vm.run_function(entry, Vec::new()).unwrap();
                instrs = vm.instructions_executed;
            },
            3,
        );
        println!(
            "  fibonacci({:<3}) compile: {:.3} ms  execution: {:.3} ms  instructions: {}",
            n, compile_ms, exec_ms, instrs
        );
    }
}
