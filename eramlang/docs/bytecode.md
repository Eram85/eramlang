# Bytecode Reference

EramLang bytecode is a flat list of `Instr` per function
(`src/bytecode.rs`). A `Program` is a table of `Function`s; each `Function`
owns its own constant pool, its own instruction stream, and (for
introspection / the debugger) a `local_names` table mapping slot index to
the original source name.

## Instruction set

| Instruction | Stack effect | Meaning |
|---|---|---|
| `Const(i)` | `-> v` | push `constants[i]` |
| `ConstVoid` | `-> void` | push the void value |
| `Pop` | `v ->` | discard top of stack |
| `Dup` | `v -> v v` | duplicate top of stack |
| `Add Sub Mul Div Mod` | `a b -> r` | arithmetic (int/float, `+` also does string concat) |
| `Neg` | `a -> r` | unary negation |
| `Not` | `a -> r` | logical not |
| `Eq Neq Lt Gt Le Ge` | `a b -> bool` | comparisons |
| `And Or` | `a b -> bool` | logical and/or |
| `LoadLocal(i)` / `StoreLocal(i)` | | read/write local slot `i` in the current frame |
| `Jump(off)` | | unconditional jump to absolute instruction offset |
| `JumpIfFalse(off)` / `JumpIfTrue(off)` | `v ->` | conditional jump, consumes the condition |
| `Call(fn_idx, argc)` | `args... -> ret` | call a user function by table index |
| `CallBuiltin(name, argc)` | `args... -> ret` | call a stdlib function by name |
| `Return` / `ReturnVoid` | | pop current frame, push return value to caller |
| `MakeArray(n)` | `items... -> arr` | build an array from the top `n` stack values |
| `IndexGet` / `IndexSet` | | array/string element read or write |
| `MakeStruct(name, fields)` | `values... -> struct` | build a struct value |
| `GetField(name)` / `SetField(name)` | | struct field read/write |
| `MakeEnum(enum, variant)` | `-> enum_value` | construct a tagged enum value |
| `Print(nl)` | `v ->` | (reserved; the current lowering emits `CallBuiltin("print"/"println", ...)` instead) |
| `Halt` | | stop execution |

## Example

```eram
let x = 10 + 20;
print(x);
```

Because of constant folding in the optimizer (see `optimizer.rs`), this
compiles to:

```
Const(0)              ; constants[0] = 30  (folded from 10 + 20)
StoreLocal(0)
Const(0)               ; constant-propagated: reuses the same constant, not LoadLocal
CallBuiltin("print", 1)
```

You can see this yourself:

```bash
eram dump-bytecode examples/hello.er
eram dump-ir examples/hello.er
```

## Program layout

Every EramLang program compiles to a `bytecode::Program` containing:

- One `Function` per user-defined `fn`.
- One synthetic `__script__` function holding the compiled top-level
  statements. If the program declares `fn main()`, `__script__` calls it
  automatically after running any top-level statements — this is how both
  "script style" programs (top-level `let`/`print`) and "`fn main()`
  style" programs both work without special-casing either in the VM.
