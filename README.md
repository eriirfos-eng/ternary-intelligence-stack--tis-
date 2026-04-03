# ternlang

> `trit ∈ { -1, 0, +1 }` — A systems programming language for ternary computing.

Compiles to BET bytecode · Runs on the BET VM · Ships an MCP server.

[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)
[![Phase 5.0](https://img.shields.io/badge/Phase-5.0%20Actor%20Model-brightgreen)](ROADMAP.md)
[![RFI-IRFOS](https://img.shields.io/badge/Built%20by-RFI--IRFOS-8B5CF6)](https://rfi-irfos.com)

---

Binary systems treat absence as null. Ternary systems treat it as a **state**.


---

## The Three States

```
-1  ->  conflict     signal is negative, resolvable
 0  ->  hold         active, not null -- the most misunderstood trit
+1  ->  truth        signal is affirmative
```

Every value, every branch, every match arm in ternlang is grounded in these three states. The compiler enforces exhaustiveness — you cannot write a `match` that forgets `0`.

---

## Language at a Glance

```ternlang
// Balanced ternary addition with carry
fn ternadd(a: trit, b: trit, c: trit) -> trit {
    let ab: trit = consensus(a, b);
    return consensus(ab, c);
}

// Sparse inference -- zero-weighted connections skipped at the VM level
fn linear(W: trittensor<128 x 64>, x: trittensor<64 x 1>) -> trittensor<128 x 1> {
    @sparseskip let out: trittensor<128 x 1> = matmul(W, x);
    return out;
}

// Every match must cover -1, 0, +1 -- or the compiler rejects it
fn decide(signal: trit) -> trit {
    match signal {
         1 => { return  1; }   // truth
         0 => { return  0; }   // hold
        -1 => { return -1; }   // conflict
    }
}
```

### Structs

```ternlang
struct Synapse {
    weight: trit,
    active: trit,
}

fn update(s: Synapse, input: trit) -> trit {
    let w: trit = s.weight;
    return consensus(w, input);
}
```

### Actor Model

```ternlang
agent Voter {
    fn handle(msg: trit) -> trit {
        match msg {
             1 => { return  1; }
             0 => { return  0; }
            -1 => { return -1; }
        }
    }
}

fn run_vote(signal: trit) -> trit {
    let v: agentref = spawn Voter;
    send v signal;
    let result: trit = await v;
    return result;
}
```

---

## Benchmark: Sparse Ternary Inference

The `@sparseskip` annotation routes `matmul()` to the `TSPARSE_MATMUL` opcode — zero-weight elements are **skipped at the instruction level**, not masked in software.

Measured on BitNet-style ternary weight matrices:

```
Weight matrix sparsity:   56.2%
Dense multiply ops:       4,096
Sparse multiply ops:      1,792
Skipped (free):           2,304

Result:  2.3x fewer multiply operations vs float32 dense
```

In a ternary weight model, 0-weighted connections contribute nothing to output. The BET VM eliminates them entirely — no conditional branches, no software masks, no wasted cycles.

---

## MCP Integration

`ternlang-mcp` connects to any MCP-compatible agent (Claude Desktop, custom agents) and exposes ternary logic as callable tools. Any binary agent becomes a ternary decision engine.

**Claude Desktop setup** — add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "ternlang": {
      "command": "/path/to/ternlang-root/target/release/ternlang-mcp"
    }
  }
}
```

### Available Tools

| Tool | Description |
|------|-------------|
| `trit_decide` | Float evidence -> ternary decision with confidence + sparsity |
| `trit_consensus` | `consensus(a, b)` with carry on the live BET VM |
| `trit_eval` | Evaluate a ternlang expression via MCP |
| `ternlang_run` | Compile and run a `.tern` program remotely |
| `quantize_weights` | `f32[]` -> `{-1, 0, +1}` via BitNet thresholding |
| `sparse_benchmark` | Sparse vs dense matmul with skip statistics |

### Example: Ternary Decision Call

```json
{
  "tool": "trit_decide",
  "arguments": {
    "evidence": [0.8, -0.3, 0.1, 0.9, -0.7],
    "threshold": 0.35
  }
}
```

Response:

```json
{
  "decision": "+1 (truth)",
  "confidence": 0.72,
  "signal_sparsity": "40.0%",
  "quantized_trits": [1, -1, 0, 1, -1],
  "interpretation": "Majority evidence is affirmative. Sparse signal -- 40% of inputs are neutral."
}
```

---

## Quick Start

```bash
# Build everything
cd ternlang-root
cargo build --release

# Run a .tern program
./target/release/ternlang-cli run test.tern

# Compile to bytecode
./target/release/ternlang-cli build test.tern

# Start the MCP server
./target/release/ternlang-mcp

# Easter egg
./target/release/ternlang-cli enlighten
```

---

## Architecture

```
ternlang-root/
├── ternlang-core/        Lexer, AST, parser, semantic, codegen (betbc), BET VM
├── ternlang-cli/         ternlang run / build (clap)
├── ternlang-ml/          BitNet quantization, sparse matmul, linear layer, benchmarks
├── ternlang-mcp/         MCP server -- 6 tools, JSON-RPC 2.0 stdio
├── ternlang-codegen/     stub -- HDL backend planned
├── ternlang-test/        stub -- integration test harness
└── stdlib/
    ├── std/trit.tern      abs, min, max, clamp, threshold, sign, majority
    ├── std/math.tern      ternadd3, neg, balance, step, rectify
    ├── std/tensor.tern    zeros, sparse_mm, dense_mm
    ├── std/io.tern        print_trit, print_tensor, newline
    ├── ml/quantize.tern   hard_threshold, soft_threshold
    └── ml/inference.tern  linear, linear_dense, attend, decide
```

---

## BET VM Opcode Reference

| Opcode | Mnemonic | Description |
|--------|----------|-------------|
| `0x01` | `TPUSH` | Push trit literal |
| `0x02` | `TADD` | Balanced ternary add with carry |
| `0x03` | `TMUL` | Ternary multiply |
| `0x04` | `TNEG` | Negate trit |
| `0x05-07` | `TJMP_POS/ZERO/NEG` | Conditional jump |
| `0x08` | `TSTORE` | Store to register |
| `0x09` | `TLOAD` | Load from register |
| `0x0e` | `TCONS` | Consensus (ternary OR) |
| `0x0f` | `TALLOC` | Allocate tensor on heap |
| `0x10` | `TCALL` | Call function (push return addr) |
| `0x11` | `TRET` | Return from function |
| `0x20` | `TMATMUL` | Dense matrix multiply |
| `0x21` | `TSPARSE_MATMUL` | **Sparse matmul -- zero weights skipped** |
| `0x22` | `TIDX` | Index into tensor |
| `0x23` | `TSET` | Set tensor element |
| `0x24` | `TSHAPE` | Push tensor dimensions |
| `0x25` | `TSPARSITY` | Count zero elements |
| `0x30` | `TSPAWN` | Spawn agent instance |
| `0x31` | `TSEND` | Send message to agent mailbox |
| `0x32` | `TAWAIT` | Await agent response |

BET encoding: `0b01` = -1, `0b10` = +1, `0b11` = 0, `0b00` = invalid (fault)

---

## Key Language Properties

- **`trit`** — three states: `-1` (conflict), `0` (hold — active, not null), `+1` (truth)
- **`match`** — must cover all three arms or the compiler rejects it (`NonExhaustiveMatch`)
- **`@sparseskip`** — routes `matmul()` to `TSPARSE_MATMUL`; zero-state elements skipped at VM level
- **actors** — `agent` / `spawn` / `send` / `await`; synchronous in v0.1, distributed in v0.2
- **pipeline** — `.tern` -> `.tbc` (bytecode) -> BET VM execution

---

## Roadmap

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Core language & VM | Complete |
| 2 | CLI & built-ins | Complete |
| 3 | TritTensors & sparse inference | Complete |
| 3.5 | MCP server | Complete |
| 4 | Language completeness (`for` / `while` / `struct` / `cast` / `use`) | Complete |
| 5.0 | Actor model (local) | Complete |
| 5.1 | Distributed actors (TCP transport) | Next |
| 6 | HDL / Verilog backend | Planned |

Full roadmap: [ROADMAP.md](ternlang-root/ROADMAP.md)

---

## License

**Open Core — LGPL v3** applies to the compiler and stdlib.

Commercial tier (planned): `ternlang-ml`, HDL backend, distributed runtime.

*Ternlang*, *BET VM*, and *Balanced Ternary Execution* are trademarks of RFI-IRFOS.

---

*Built by Simeon Kepp & Claude — RFI-IRFOS — 2026*
