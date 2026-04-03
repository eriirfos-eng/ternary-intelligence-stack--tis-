# BET ISA Specification v0.1
## Balanced Ternary Execution — Instruction Set Architecture

**RFI-IRFOS Ternary Intelligence Stack (TIS)**  
*Document version:* 0.1  
*Status:* Draft standard

---

## Abstract

The Balanced Ternary Execution (BET) ISA defines a stack-based instruction set for a virtual machine operating natively on balanced ternary values. Each computational primitive is grounded in the three-state logic `T = {-1, 0, +1}` where 0 is an active computational state ("hold"), not null.

This document defines:
1. Trit encoding and wire representation
2. Machine model (registers, stack, heap, call stack)
3. Full instruction set with semantics and encoding
4. Actor model extension (Phase 5)
5. Tensor operations extension (Phase 3)
6. Hardware mapping (Phase 6)

---

## 1. Trit Encoding

### 1.1 Symbolic Values

| Symbol | Value | Semantics |
|--------|-------|-----------|
| truth  | +1    | Affirmative, positive assertion |
| hold   |  0    | Active unresolved state (NOT null) |
| conflict | -1  | Negative, contradictory |

### 1.2 2-Bit Packed Encoding

BET uses 2-bit packed encoding for efficient storage and wire transport:

| Bit Pattern | Trit Value | Semantics |
|-------------|-----------|-----------|
| `0b10`      | +1        | truth |
| `0b11`      |  0        | hold |
| `0b01`      | -1        | conflict |
| `0b00`      | FAULT     | Invalid state — trap |

Multiple trits are packed into bytes in little-endian order. One byte holds up to 4 trits (the upper bits are padding if not a multiple of 4).

### 1.3 Ternary Arithmetic

**Addition** (carries into the next position when magnitude exceeds 1):

| a  | b  | sum | carry |
|----|-----|-----|-------|
| +1 | +1 | -1  | +1    |
| +1 |  0 | +1  |  0    |
| +1 | -1 |  0  |  0    |
|  0 |  0 |  0  |  0    |
|  0 | -1 | -1  |  0    |
| -1 | -1 | +1  | -1    |

**Multiplication** (sign rule):

| a  | b  | a×b |
|----|-----|-----|
| +1 | +1 | +1  |
| +1 | -1 | -1  |
| -1 | -1 | +1  |
|  0 | any|  0  |

**Consensus** (ternary OR — agrees with both, holds otherwise):

| a  | b  | consensus(a,b) |
|----|-----|----------------|
| +1 | +1 | +1 |
| -1 | -1 | -1 |
| +1 | -1 |  0 |
| +1 |  0 | +1 |
| -1 |  0 | -1 |
|  0 |  0 |  0 |

---

## 2. Machine Model

### 2.1 Registers

- **27 general-purpose ternary registers** (R0–R26), each storing one `Value`
- **Carry register** (CR): holds the carry trit from the last addition
- All registers reset to hold (0) on VM initialization

### 2.2 Value Types

| Type | Description |
|------|-------------|
| `Trit` | A single balanced ternary value: -1, 0, or +1 |
| `Int` | A 64-bit signed integer (used for tensor indices) |
| `TensorRef(N)` | Reference to tensor N in the heap |
| `AgentRef(N)` | Reference to agent instance N in the agent table |

### 2.3 Stack

An unbounded operand stack. All instructions consume and produce values from/to this stack. Values are typed; type mismatches produce `TypeMismatch` runtime errors.

### 2.4 Call Stack

A separate return-address stack used by `TCALL`/`TRET`. When the call stack is empty, `TRET` halts the VM.

### 2.5 Tensor Heap

A flat array of `Vec<Trit>` slices. Tensors are referenced by index (`TensorRef`). All tensors are stored in row-major flat form; square dimensions are inferred from length.

### 2.6 Agent Table

A registry of `AgentInstance { handler_addr, mailbox: VecDeque<Value> }`. Agents are spawned by `TSPAWN` and referenced by `AgentRef`.

---

## 3. Instruction Set

### 3.1 Notation

- **Stack effect:** `( before -- after )` — left is bottom, right is top
- **PC:** program counter
- All addresses are 16-bit unsigned integers (little-endian, `u16`)

---

### 3.2 Core Instructions

#### `0x00` — THALT
```
( -- )
```
Terminate VM execution. Returns `Ok(())`.

---

#### `0x01` — TPUSH `<packed: u8>`
```
( -- Trit )
```
Push a trit literal. The byte following the opcode is the packed trit (2-bit BET encoding in bits [1:0]).

---

#### `0x02` — TADD
```
( Trit[a] Trit[b] -- Trit[sum] )
```
Balanced ternary addition. Writes carry to CR.

---

#### `0x03` — TMUL
```
( Trit[a] Trit[b] -- Trit[a×b] )
```
Balanced ternary multiply. No carry.

---

#### `0x04` — TNEG
```
( Trit[a] -- Trit[-a] )
```
Negate: maps +1↔-1, 0→0.

---

#### `0x05` — TJMP_POS `<addr: u16>`
```
( Trit -- )
```
Pop trit. If +1, jump to `addr`; otherwise fall through.

---

#### `0x06` — TJMP_ZERO `<addr: u16>`
```
( Trit -- )
```
Pop trit. If 0 (hold), jump to `addr`.

---

#### `0x07` — TJMP_NEG `<addr: u16>`
```
( Trit -- )
```
Pop trit. If -1, jump to `addr`.

---

#### `0x08` — TSTORE `<reg: u8>`
```
( Value -- )
```
Pop value and write to register `reg`.

---

#### `0x09` — TLOAD `<reg: u8>`
```
( -- Value )
```
Push value from register `reg`.

---

#### `0x0a` — TDUP
```
( Value -- Value Value )
```
Duplicate top of stack.

---

#### `0x0b` — TJMP `<addr: u16>`
```
( -- )
```
Unconditional jump to `addr`.

---

#### `0x0c` — TPOP
```
( Value -- )
```
Discard top of stack.

---

#### `0x0d` — TLOADCARRY
```
( -- Trit )
```
Push carry register (CR) onto stack.

---

#### `0x0e` — TCONS
```
( Trit[a] Trit[b] -- Trit[consensus(a,b)] )
```
Ternary consensus (OR). Agrees with both operands when equal; else hold.

---

#### `0x0f` — TALLOC `<size: u16>`
```
( -- TensorRef )
```
Allocate a tensor of `size` trits (zero-initialized to hold). Push `TensorRef(N)`.

---

#### `0x10` — TCALL `<addr: u16>`
```
( -- )
```
Push return address to call stack. Jump to `addr`.

---

#### `0x11` — TRET
```
( -- )
```
Pop return address from call stack and jump to it. If call stack is empty, halt.

---

### 3.3 Tensor Operations

#### `0x20` — TMATMUL
```
( TensorRef[A] TensorRef[B] -- TensorRef[result] )
```
Dense matrix multiply. Both tensors must be square with matching dimensions.

---

#### `0x21` — TSPARSE_MATMUL ⭐
```
( TensorRef[A] TensorRef[B] -- TensorRef[result] Int[skipped] )
```
**Sparse matrix multiply.** Zero-state elements in B are skipped entirely — no multiply issued, no accumulation. Pushes result tensor ref and the count of skipped operations for observability.

This is the flagship BET operation: **56% weight sparsity → 2.3× fewer multiply operations** in practice (BitNet-style ternary weight matrices).

---

#### `0x22` — TIDX
```
( TensorRef Int[row] Int[col] -- Trit )
```
Index into tensor at (row, col). Returns element trit.

---

#### `0x23` — TSET
```
( TensorRef Int[row] Int[col] Trit -- )
```
Set tensor element at (row, col) to trit. Mutates in place.

---

#### `0x24` — TSHAPE
```
( TensorRef -- Int[rows] Int[cols] )
```
Push tensor dimensions. Current implementation assumes square tensors.

---

#### `0x25` — TSPARSITY
```
( TensorRef -- Int[zero_count] )
```
Count zero-state (hold) elements in tensor.

---

### 3.4 Actor Operations

#### `0x30` — TSPAWN `<type_id: u16>`
```
( -- AgentRef )
```
Create a new agent instance of type `type_id`. The type_id maps to a registered handler address. Push `AgentRef(N)` for the new instance.

---

#### `0x31` — TSEND
```
( AgentRef Value -- )
```
Enqueue `Value` in the agent's mailbox. No blocking; returns immediately.

---

#### `0x32` — TAWAIT
```
( AgentRef -- Value )
```
Pop `AgentRef`. Pop the front message from its mailbox (or hold if empty). Push message onto operand stack. TCALL the handler address. The handler's `TRET` returns the result to the caller.

---

## 4. Error Conditions

| Error | Cause |
|-------|-------|
| `StackUnderflow` | Pop from empty stack |
| `TypeMismatch` | Wrong value type for instruction |
| `InvalidOpcode(N)` | Unknown byte `N` |
| `InvalidRegister(N)` | Register index ≥ 27 |
| `PcOutOfBounds(N)` | PC exceeds code length |
| `BetFault` | Invalid 2-bit encoding `0b00` |

---

## 5. Hardware Mapping

### 5.1 Wire Pair Encoding (FPGA/ASIC)

Each trit maps to a 2-bit wire pair `[t1:t0]`:

| t1 | t0 | Trit |
|----|-----|------|
| 0  | 1   | -1 (conflict) |
| 1  | 0   | +1 (truth) |
| 1  | 1   |  0 (hold) |
| 0  | 0   | FAULT |

A `trittensor<N x M>` becomes a `[N*M*2-1:0]` bus.

### 5.2 BET Processor Components

| Module | Function |
|--------|----------|
| `bet_regfile` | 27 × 2-bit register file, synchronous write, reset to hold |
| `bet_pc` | 16-bit program counter with jump-load |
| `bet_control` | Opcode → control signals (single-cycle) |
| `bet_alu` | ADD/MUL/NEG/CONS with carry |
| `trit_add` | Half-adder with carry, Verilog case statement |
| `trit_mul` | Multiply with zero-skip detect |
| `trit_neg` | Bit-swap inversion |
| `trit_cons` | Equality-based consensus |
| `sparse_matmul(N)` | N×N array with per-cell zero-skip enable (clock gating) |

### 5.3 Sparse Matmul Array

The `sparse_matmul_NxN` Verilog module instantiates N² `trit_mul` cells, each with a `skip` enable signal:

```verilog
assign skip[i][j] = (w_ij == 2'b11); // zero weight = hold = skip
trit_mul u_mul (
    .a(a_j),
    .b(skip[i][j] ? 2'b11 : w_ij),
    .y(prod[i][j])
);
```

At 56% sparsity, 56% of the multiply cells are clock-gated per cycle — directly translating to power savings on FPGA and ASIC targets.

---

## 6. Extension Points

The opcode space reserves:
- `0x12`–`0x1f`: future core instructions
- `0x26`–`0x2f`: future tensor instructions  
- `0x33`–`0x3f`: future actor instructions
- `0x40`–`0xff`: user-defined / implementation-specific

---

## 7. Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-04-03 | Initial draft. Core ISA (0x00–0x0e), tensor ops (0x0f, 0x20–0x25), actor ops (0x10–0x11, 0x30–0x32). |

---

*Specification maintained by RFI-IRFOS.*  
*Implementation: [ternlang-root](https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-)*  
*Built by Simeon Kepp & Claude — 2026*
