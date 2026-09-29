# Compiler Internals: AST -> IR -> Bytecode

This document covers the two lowering stages implemented in
`src/compiler.rs`: `IrGen` (AST -> IR) and `CodeGen` (IR -> bytecode).

## Stage 1: `IrGen` (AST -> IR)

`IrGen` walks the AST once, maintaining:

- A stack of scopes (`Vec<HashMap<String, String>>`) mapping each source
  variable name to a **unique** generated name (`x$1`, `x$2`, ...). This is
  what makes shadowing work correctly: `let x = 10; let x = x + 20;`
  resolves the right-hand `x` to `x$1` (the outer binding) before
  registering the new binding as `x$2`.
- A label counter, for generating unique symbolic jump targets
  (`while_start_3`, `endif_7`, ...).
- A loop stack of `(continue_label, break_label)` pairs, so `break`/`continue`
  compile to a plain `Jump` to the right label without the IR needing to
  know anything about loop nesting.

### Desugaring during IR generation

A few constructs are desugared directly into simpler IR rather than given
their own instructions:

- **`for i in a..b { body }`** becomes a `while`-shaped loop: evaluate `a`
  and `b` once into locals, loop while `i < b`, and increment `i` at a
  `continue` label positioned *after* the body (so `continue` still runs
  the increment).
- **Logical `&&` / `||`** compile to short-circuiting jumps (`JumpIfFalse`
  / `JumpIfTrue`) rather than always evaluating both sides.
- **Compound assignment to an array element or struct field**
  (`arr[i] += 1`, `s.field *= 2`) evaluates the target's address components
  into temporary locals so they're evaluated exactly once, then reads,
  combines, and writes through those temporaries — avoiding the need for
  extra stack-manipulation instructions (`Dup`/`Swap` beyond what already
  exists).
- **`match`** stores the subject in a temporary local, then compiles each
  arm as `LoadLocal(subject); MakeEnum(enum, variant); Eq; JumpIfFalse(next)`
  followed by the arm body and a jump to a shared end label.

## Stage 2: `CodeGen` (IR -> Bytecode)

`CodeGen::lower_function` runs two passes over one function's IR:

1. **Label resolution.** Strip every `IrOp::Label` out of the stream while
   recording its position (its *final* instruction index, since labels
   themselves emit no instruction). This gives an offset for every symbolic
   jump target before any bytecode is emitted.
2. **Instruction lowering.** Walk the label-free stream, translating each
   `IrOp` into the corresponding `bytecode::Instr`. `LoadLocal`/`StoreLocal`
   names are resolved to slot indices via a `HashMap<String, usize>` that
   assigns indices on first sight — function parameters are inserted first,
   so they always land in slots `0..arity`, matching the VM's calling
   convention (arguments are placed into the first `arity` local slots).

### Two-pass function-call resolution

A function's own bytecode can't know another function's table index until
every function has been lowered (a function can call one declared later in
the source). `CodeGen::lower` therefore lowers every function's body first
with a **placeholder** `CallBuiltin("__fn__<name>", argc)` instruction for
user-function calls, then `resolve_calls` does a second pass over the whole
program rewriting every placeholder into a real `Instr::Call(index, argc)`
once the function table is complete. Genuine builtin calls
(`CallBuiltin("print", ...)`) are unaffected — the `__fn__` prefix can't
collide with a real builtin name.

### The `__script__` function

Top-level statements (outside of any `fn`) are compiled into a synthetic
function named `__script__`, exactly like a zero-parameter `fn`. If the
program declares `fn main()`, `IrGen` appends a `Call("main", 0); Pop` to
the end of the top-level IR, so `__script__` always contains the program's
full entry-point behavior whether the person wrote "script style" top-level
code or a `fn main()`. The VM never needs to special-case either style; it
just calls `__script__`.
