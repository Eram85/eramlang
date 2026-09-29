# EramLang Language Reference

## Comments

```eram
// line comment
/* block
   comment */
```

## Variables

```eram
let name = "Eram";      // immutable
let mut score = 10;     // mutable
score = score + 5;

let x = 10;
let x = x + 20;          // shadowing: rebinds `x` in the same scope
```

Assigning to an immutable `let` is a semantic error caught at compile time
(`eram check`), not a runtime failure.

## Types

`int`, `float`, `bool`, `string`, `char`, `void`, and array types written
`[T]` internally (array *literals* don't need an explicit type annotation;
the element type is inferred).

## Operators

| Category | Operators |
|---|---|
| Arithmetic | `+ - * / %` |
| Comparison | `== != < > <= >=` |
| Logical | `&& \|\| !` (short-circuiting) |
| Assignment | `= += -= *= /=` |

Standard precedence applies (`*` `/` `%` bind tighter than `+` `-`, which
bind tighter than comparisons, which bind tighter than `&&`, which binds
tighter than `\|\|`).

## Control flow

```eram
if age >= 18 { print("Adult"); } else { print("Minor"); }

while i < 10 { print(i); i += 1; }

for i in 0..10 { print(i); }   // exclusive of the upper bound

while true { break; }
for i in 0..10 { if i == 5 { continue; } print(i); }
```

## Functions

```eram
fn add(a: int, b: int) -> int {
    return a + b;
}
```

Functions support parameters, return values, recursion, local variables,
and nested calls. A function with no `-> T` returns `void`.

## Arrays

```eram
let numbers = [10, 20, 30, 40];
print(numbers[0]);
numbers[1] = 50;
len(numbers);
push(numbers, 100);
pop(numbers);
```

## Strings

```eram
let name = "Eram";
print(name);
print(len(name));
let message = "Hello " + name; // concatenation
```

## Structs

```eram
struct User {
    name: string,
    age: int,
}

let user = User { name: "Eram", age: 23 };
print(user.name);
```

## Enums and pattern matching

```eram
enum Status {
    Success,
    Failure,
}

fn describe(s: Status) -> string {
    return match s {
        Success => "it worked",
        Failure => "it failed",
    };
}
```

`match` arms may use `=>` followed by either a single expression or a
`{ ... }` block. A non-exhaustive `match` produces a compile-time warning,
not an error, since EramLang's enums don't (yet) carry data and adding a
future variant is safe to defer to v0.2's richer pattern matching.

Note: EramLang functions and blocks require an explicit `return` — there is
no implicit "last expression is the return value" rule for *function*
bodies (unlike `match` arms, which do evaluate to a value). So:

```eram
fn describe(s: Status) -> string {
    match s { Success => "ok", Failure => "bad" }   // ERROR: missing `;`/`return`
}
fn describe(s: Status) -> string {
    return match s { Success => "ok", Failure => "bad" };  // OK
}
```

## Standard library

`print`, `println`, `input`, `len`, `push`, `pop`, `sqrt`, `abs`, `min`,
`max`. See [`../src/stdlib.rs`](../src/stdlib.rs).
