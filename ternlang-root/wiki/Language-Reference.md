# Ternlang Language Reference

Ternlang is a statically-typed, balanced ternary programming language that compiles to BET (Balanced Ternary Execution) bytecode.

---

## Types

| Type | Description | Values |
|------|-------------|--------|
| `trit` | Single balanced trit | −1, 0, +1 |
| `trittensor[N×M]` | N×M ternary matrix | Matrix of trits |
| `bool` | Boolean (castable to trit) | true/false |
| `int` | Integer | Standard integers |
| `float` | Float | Standard floats |
| `string` | String literal | UTF-8 |
| `agentref` | Reference to a spawned agent | — |

---

## Literals

```ternlang
let a: trit = 1;      // +1 (affirm)
let b: trit = 0;      // 0  (tend)
let c: trit = -1;     // -1 (reject)
```

---

## Operators

| Operator | Name | Description |
|----------|------|-------------|
| `consensus(a, b)` | Consensus | Ternary addition: truth+conflict=hold |
| `invert(x)` | Invert | Negate: maps +1↔−1, 0→0 |
| `mul(a, b)` | Multiply | Ternary multiplication |
| `truth()` | Truth literal | Returns +1 |
| `hold()` | Hold literal | Returns 0 |
| `conflict()` | Conflict literal | Returns −1 |

---

## Control Flow

### Three-way match (exhaustive)

```ternlang
match x {
    -1 => { /* reject branch */ }
     0 => { /* tend branch   */ }
     1 => { /* affirm branch */ }
}
```

All three arms are required. The compiler enforces exhaustiveness.

### If-ternary

```ternlang
if condition ? {
    // affirm
} else {
    // reject
} else {
    // tend
}
```

### Loops

```ternlang
for item in collection { }
while condition ? { } else { } else { }
loop { if done { break; } }
```

---

## Functions

```ternlang
fn decide(a: trit, b: trit) -> trit {
    return consensus(a, b);
}
```

---

## Sparse Inference

```ternlang
fn run_layer(W: trittensor[128×64], input: trittensor[1×128]) -> trittensor[1×64] {
    @sparseskip
    return matmul(input, W);   // emits TSPARSE_MATMUL — skips zero weights
}
```

The `@sparseskip` directive instructs the compiler to emit `TSPARSE_MATMUL` instead of `TMATMUL` for the next matrix multiplication. Zero-weight multiplications are skipped at the bytecode level — not approximated, provably zero.

---

## Structs

```ternlang
struct Agent {
    confidence: trit,
    state: trit,
}

let a = Agent { confidence: 1, state: 0 };
let c = a.confidence;
a.state = -1;
```

---

## Actors

```ternlang
agent Classifier {
    fn handle(msg: trit) -> trit {
        return invert(msg);
    }
}

let ref = spawn Classifier;
send ref 1;
let result = await ref;   // → -1
```

---

## Modules and Use

```ternlang
use std::trit;
use ml::quantize;
```

---

## EBNF Grammar

Full grammar: [`spec/grammar.ebnf`](../spec/grammar.ebnf)
Language dictionary: [`spec/ternlang-dictionary-v0.1.json`](../spec/ternlang-dictionary-v0.1.json)
