//! Integration tests: compile and run complete `.er` programs and verify
//! their behavior end-to-end through the real pipeline (lex -> parse ->
//! semantic analysis -> IR -> bytecode -> VM). These exercise the compiler
//! the same way the CLI's `eram run` does.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn eram_bin() -> &'static str {
    env!("CARGO_BIN_EXE_eram")
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn unique_path(prefix: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("eram_{}_{}_{}.er", prefix, std::process::id(), n))
}

fn run_source(src: &str) -> (String, String, bool) {
    let path = unique_path("run");
    std::fs::write(&path, src).unwrap();
    let output = Command::new(eram_bin())
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run eram");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    )
}

fn check_source(src: &str) -> (String, String, bool) {
    let path = unique_path("check");
    std::fs::write(&path, src).unwrap();
    let output = Command::new(eram_bin())
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run eram");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    )
}

#[test]
fn test_hello_world() {
    let (out, _err, ok) = run_source(r#"fn main() { println("Hello from EramLang!"); }"#);
    assert!(ok);
    assert_eq!(out, "Hello from EramLang!\n");
}

#[test]
fn test_final_demo_program() {
    let src = r#"
struct User {
    name: string,
    age: int,
}

fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}

fn main() {
    let user = User {
        name: "Eram",
        age: 23,
    };

    println("Hello " + user.name);

    let result = factorial(5);

    println("Factorial:");
    println(result);

    for i in 0..5 {
        println(i);
    }
}
"#;
    let (out, err, ok) = run_source(src);
    assert!(ok, "stderr: {}", err);
    assert_eq!(out, "Hello Eram\nFactorial:\n120\n0\n1\n2\n3\n4\n");
}

#[test]
fn test_operator_precedence() {
    let (out, _err, ok) = run_source("let result = 10 + 5 * 2;\nprint(result);");
    assert!(ok);
    assert_eq!(out, "20");
}

#[test]
fn test_variable_shadowing() {
    let (out, _err, ok) = run_source("let x = 10;\nlet x = x + 20;\nprint(x);");
    assert!(ok);
    assert_eq!(out, "30");
}

#[test]
fn test_mutable_reassignment() {
    let (out, _err, ok) = run_source("let mut score = 10;\nscore = score + 5;\nprint(score);");
    assert!(ok);
    assert_eq!(out, "15");
}

#[test]
fn test_compound_assign() {
    let (out, _err, ok) = run_source("let mut i = 0;\nwhile i < 5 { i += 1; }\nprint(i);");
    assert!(ok);
    assert_eq!(out, "5");
}

#[test]
fn test_fibonacci_recursion() {
    let src = r#"
fn fibonacci(n: int) -> int {
    if n <= 1 { return n; }
    return fibonacci(n - 1) + fibonacci(n - 2);
}
print(fibonacci(10));
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "55");
}

#[test]
fn test_for_loop_break_continue() {
    let src = r#"
for i in 0..10 {
    if i == 5 { continue; }
    if i == 8 { break; }
    print(i);
}
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "0123467");
}

#[test]
fn test_arrays() {
    let src = r#"
let mut numbers = [10, 20, 30, 40];
print(numbers[0]);
numbers[1] = 50;
print(numbers[1]);
print(len(numbers));
push(numbers, 100);
print(len(numbers));
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "105045");
}

#[test]
fn test_strings() {
    let (out, _err, ok) =
        run_source(r#"let name = "Eram"; print(len(name)); print("Hello " + name);"#);
    assert!(ok);
    assert_eq!(out, "4Hello Eram");
}

#[test]
fn test_structs() {
    let src = r#"
struct User { name: string, age: int }
let user = User { name: "Eram", age: 23 };
print(user.name);
print(user.age);
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "Eram23");
}

#[test]
fn test_enum_match() {
    let src = r#"
enum Status { Success, Failure }
fn describe(s: Status) -> string {
    return match s {
        Success => "ok",
        Failure => "bad",
    };
}
print(describe(Success));
print(describe(Failure));
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "okbad");
}

#[test]
fn test_stdlib_math() {
    let (out, _err, ok) =
        run_source("print(sqrt(16.0)); print(abs(-5)); print(min(3, 7)); print(max(3, 7));");
    assert!(ok);
    assert_eq!(out, "4537");
}

#[test]
fn test_division_by_zero_runtime_error() {
    let (_out, err, ok) = run_source("fn main() { let x = 1; let y = 0; print(x / y); }");
    assert!(!ok);
    assert!(err.contains("division by zero"), "stderr: {}", err);
}

#[test]
fn test_array_out_of_bounds_runtime_error() {
    let (_out, err, ok) = run_source("fn main() { let a = [1,2,3]; print(a[10]); }");
    assert!(!ok);
    assert!(err.contains("out of bounds"), "stderr: {}", err);
}

#[test]
fn test_undefined_variable_semantic_error() {
    let (_out, err, ok) = check_source("fn main() { print(x); }");
    assert!(!ok);
    assert!(err.contains("undefined variable"), "stderr: {}", err);
}

#[test]
fn test_type_mismatch_error() {
    let (_out, err, ok) = check_source(r#"fn main() { let x: int = "hello"; print(x); }"#);
    assert!(!ok);
    assert!(err.contains("expected `int`"), "stderr: {}", err);
}

#[test]
fn test_wrong_arg_count_error() {
    let (_out, err, ok) =
        check_source("fn add(a: int, b: int) -> int { return a + b; }\nfn main() { add(1); }");
    assert!(!ok);
    assert!(err.contains("expects 2 argument"), "stderr: {}", err);
}

#[test]
fn test_immutable_assignment_error() {
    let (_out, err, ok) = check_source("fn main() { let x = 5; x = 6; }");
    assert!(!ok);
    assert!(err.contains("immutable"), "stderr: {}", err);
}

#[test]
fn test_break_outside_loop_error() {
    let (_out, err, ok) = check_source("fn main() { break; }");
    assert!(!ok);
    assert!(err.contains("break"), "stderr: {}", err);
}

#[test]
fn test_check_passes_on_valid_program() {
    let (out, err, ok) = check_source("fn main() { let x = 5; print(x); }");
    assert!(ok, "stderr: {}", err);
    assert!(out.contains("check passed"));
}

#[test]
fn test_comments_are_ignored() {
    let src = r#"
// line comment
/* block
   comment */
print(42); // trailing comment
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "42");
}

#[test]
fn test_logical_operators_short_circuit_and_normal() {
    let src = r#"
print(true && false);
print(true || false);
print(!true);
print(1 < 2 && 3 > 2);
"#;
    let (out, _err, ok) = run_source(src);
    assert!(ok);
    assert_eq!(out, "falsetruefalsetrue");
}

#[test]
fn test_float_arithmetic() {
    let (out, _err, ok) = run_source("let x: float = 3.5; let y: float = 2.0; print(x + y);");
    assert!(ok);
    assert_eq!(out, "5.5");
}

#[test]
fn test_char_type() {
    let (out, _err, ok) = run_source("let grade: char = 'A'; print(grade);");
    assert!(ok);
    assert_eq!(out, "A");
}
