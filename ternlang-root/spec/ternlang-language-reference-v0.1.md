````markdown
# Ternlang Language Reference v0.1
### Balanced Ternary Systems Language
### Language Grammar, Dictionary, and Core Semantics

Version: 0.1  
Status: Draft Specification  
Authoring Context: RFI-IRFOS / Ternlang Core Design  
Date: 2026-04-01  

---

# 1. Introduction

Ternlang is a balanced ternary systems programming language designed for ambiguity-aware computation, sparse tensor inference, and distributed actor-based execution.

Unlike traditional binary languages built on `{0,1}`, Ternlang is built upon the balanced ternary state space:

```math
T = {-1, 0, +1}
````

Where:

* `-1` = conflict / inverse / disconfirmation
* `0` = hold / anticipation / unresolved
* `+1` = truth / assertion / intent

The neutral state `0` is not null and not an error state.

It is an active computational primitive.

This makes ambiguity a first-class concept in control flow, arithmetic, and concurrent systems.

---

# 2. Core Primitive Types

## 2.1 trit

The atomic primitive of Ternlang.

Valid values:

```ternlang
-1
0
1
```

Semantic aliases:

```ternlang
conflict = -1
hold     = 0
truth    = 1
```

Example:

```ternlang
let signal: trit = truth;
```

---

## 2.2 Standard Primitive Types

```ternlang
int
float
bool
string
```

Important:

`bool` is a compatibility type.

Core ternary logic should prefer `trit`.

---

## 2.3 Tensor Types

```ternlang
trittensor<rows x cols>
```

Example:

```ternlang
let W: trittensor<1024x1024>;
```

---

## 2.4 Distributed Types

```ternlang
agentref
nodeid
message<T>
```

Used for actor and peer-to-peer execution.

---

# 3. Reserved Keywords

The following identifiers are reserved and may not be used as variable names.

```text
let
const
fn
return
if
else
match
spawn
send
await
agent
remote
module
use
pub
struct
enum
trait
impl
loop
for
in
while
break
continue
where
as
mut
trit
trittensor
agentref
nodeid
hold
truth
conflict
sparseskip
consensus
```

---

# 4. Operators

## 4.1 Arithmetic Operators

```ternlang
+   addition
-   negation / subtraction
*   multiplication
/   safe division
```

---

## 4.2 Logical Operators

```ternlang
==  equality
!=  inequality
&&  logical and
||  logical or
```

---

## 4.3 Structural Operators

```ternlang
=   assignment
:   type annotation
->  return type
=>  branch mapping
::  namespace access
;   statement terminator
```

---

# 5. Arithmetic Semantics

---

## 5.1 Addition

Balanced consensus addition.

| a  | b  | result |
| -- | -- | ------ |
| -1 | -1 | -1     |
| -1 | 0  | -1     |
| -1 | 1  | 0      |
| 0  | -1 | -1     |
| 0  | 0  | 0      |
| 0  | 1  | 1      |
| 1  | -1 | 0      |
| 1  | 0  | 1      |
| 1  | 1  | 1      |

Example:

```ternlang
truth + conflict = hold
```

---

## 5.2 Multiplication

Carry-free sign multiplication.

| a  | b   | result |
| -- | --- | ------ |
| -1 | -1  | 1      |
| -1 | 0   | 0      |
| -1 | 1   | -1     |
| 0  | any | 0      |
| 1  | -1  | -1     |
| 1  | 0   | 0      |
| 1  | 1   | 1      |

---

## 5.3 Negation

```ternlang
-truth     = conflict
-conflict  = truth
-hold      = hold
```

---

# 6. Variable Declaration

Syntax:

```ternlang
let identifier: type = expression;
```

Examples:

```ternlang
let signal: trit = truth;
let score: int = 7;
let weights: trittensor<256x256>;
```

Mutable variables:

```ternlang
let mut signal: trit = hold;
```

---

# 7. Function Definition

Syntax:

```ternlang
fn name(param: type) -> return_type {
    ...
}
```

Example:

```ternlang
fn invert(signal: trit) -> trit {
    return -signal;
}
```

---

# 8. Control Flow

Ternlang mandates explicit ternary branching.

All trit evaluations must define all three states.

---

## 8.1 Canonical Branching

```ternlang
match signal {
    1  => { return truth; }
    0  => { return hold; }
   -1  => { return conflict; }
}
```

---

## 8.2 Compiler Rule

All branches are mandatory.

This is invalid:

```ternlang
match signal {
    1 => { ... }
   -1 => { ... }
}
```

Compiler error:

```text
missing hold-state branch
```

---

# 9. Functions and Built-ins

---

## 9.1 Core Built-ins

```ternlang
truth()
hold()
conflict()
invert(x)
consensus(a, b)
```

---

## 9.2 Example

```ternlang
let result = consensus(truth, conflict);
```

Returns:

```ternlang
hold
```

---

# 10. Tensor Language Dictionary

---

## 10.1 Allocation

```ternlang
let W = trittensor<512x512>.allocate();
```

---

## 10.2 Core Methods

```ternlang
.allocate()
.shape()
.sparsity()
.transpose()
.matmul()
.compress()
.unpack()
```

---

## 10.3 Sparse Execution Directive

```ternlang
@sparseskip
for weight in W {
    ...
}
```

Directive meaning:

Neutral `0` states are skipped physically during execution.

---

# 11. Actor Model

Ternlang uses isolated actor-based concurrency.

No shared mutable memory between agents.

---

## 11.1 Agent Definition

```ternlang
agent worker {
    fn process(x: trit) -> trit {
        return x;
    }
}
```

---

## 11.2 Spawn

```ternlang
let ref: agentref = spawn(remote_node, worker);
```

---

## 11.3 Messaging

```ternlang
send(ref, truth);
```

Receive:

```ternlang
let msg = await ref;
```

---

# 12. Namespaces

Core standard library layout:

```ternlang
std::trit
std::tensor
std::actor
std::net
std::math
std::crypto
std::io
```

Machine learning:

```ternlang
ml::quant
ml::infer
ml::sparse
ml::bitnet
```

---

# 13. Semantic Rules

---

## Rule 1

```ternlang
0 != null
```

`hold` is an active state.

---

## Rule 2

No implicit coercion from bool to trit.

Invalid:

```ternlang
let x: trit = true;
```

Valid:

```ternlang
let x: trit = cast(true);
```

---

## Rule 3

Every trit branch must be exhaustive.

---

# 14. Example Program

```ternlang
fn evaluate(signal: trit) -> trit {
    match signal {
        1  => { return truth; }
        0  => { return hold; }
       -1  => { return conflict; }
    }
}
```

---

# 15. Design Principles

Ternlang is built on five core principles:

1. ambiguity as computation
2. ternary-first logic
3. sparse AI-native execution
4. distributed actor isolation
5. hardware-targetable semantics

---

# End of Specification

Ternlang Language Reference v0.1

````
