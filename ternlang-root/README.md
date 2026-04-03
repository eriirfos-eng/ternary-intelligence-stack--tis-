# Ternlang — The Balanced Ternary Programming Language

**RFI-IRFOS Ternary Intelligence Stack (TIS)**

> *The place where the fractured ternary computing field compiles into something whole.*

[![CI](https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-/actions/workflows/rust.yml/badge.svg)](https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-/actions)
[![License: LGPL-3.0](https://img.shields.io/badge/license-LGPL--3.0-blue.svg)](LICENSE)
[![BET ISA](https://img.shields.io/badge/BET%20ISA-v0.1-brightgreen.svg)](BET-ISA-SPEC.md)

---

Balanced ternary computing — where every value is `-1`, `0`, or `+1` — has been scattered across hobbyist emulators, academic experiments, and hardware prototypes for decades. **Ternlang is the first full-stack language ecosystem for it**: compiler, VM, ML kernels, LSP, HDL backend, actor runtime, MCP integration, and package manager — all in one coherent stack.

```tern
// Three states. Every match arm required.
fn classify(signal: trit) -> trit {
    match signal {
        -1 => conflict()   // active disagreement
         0 => hold()       // active neutral — not null, not absent
        +1 => truth()      // confirmed
    }
}

// Sparse inference — skip zero-weight multiplies at the VM level
@sparseskip let output: trittensor<8 x 8> = matmul(input, weights);

// Actor model — ternary message passing
agent Classifier {
    fn handle(msg: trit) -> trit {
        classify(msg)
    }
}
let a: agentref = spawn Classifier;
send a truth();
let result: trit = await a;
```

---

## What's in the Stack

| Crate | What it does |
|---|---|
| `ternlang-core` | Lexer, parser, AST, semantic checker, BET bytecode emitter, VM |
| `ternlang-ml` | BitNet-style ternary quantization, sparse matmul, benchmarking |
| `ternlang-hdl` | Verilog-2001 codegen for FPGA — primitives, sparse matmul array, full processor |
| `ternlang-lsp` | LSP 3.17 language server — hover, completion, diagnostics for `.tern` files |
| `ternlang-mcp` | MCP server — connects any AI agent to ternary decision logic |
| `ternlang-runtime` | Distributed actor runtime — TCP transport, `TernNode`, remote `spawn`/`send`/`await` |
| `ternlang-cli` | `ternlang run/build/sim/fmt/repl` — full developer workflow |
| `ternpkg` | Package manager — `ternlang.toml`, GitHub-backed registry |

**68 tests. All green.**

---

## Quick Start

```bash
git clone https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-
cd "Ternary Intelligence Stack (TIS)/ternlang-root"
cargo build --release

# Run a .tern file
./target/release/ternlang run my_program.tern

# Interactive REPL
./target/release/ternlang repl

# Generate FPGA testbench (Icarus Verilog)
./target/release/ternlang sim my_program.tern --run

# Package manager
./target/release/ternpkg init
./target/release/ternpkg install eriirfos-eng/ternary-intelligence-stack--tis-
```

---

## The BET VM

The **Balanced Ternary Execution** VM is a stack-based processor with:

- 27 registers (each a 2-bit packed trit: `0b01`=−1, `0b10`=+1, `0b11`=0)
- Tensor heap for `trittensor<N×M>` allocation
- Full actor table: `TSPAWN` / `TSEND` / `TAWAIT`
- `TSPARSE_MATMUL` — skips zero-weight elements at the opcode level
- Formal ISA specification: [BET-ISA-SPEC.md](BET-ISA-SPEC.md)

---

## Sparse Inference

Ternary weights are naturally sparse. BitNet-style quantization collapses float weights to `{−1, 0, +1}`. The `@sparseskip` directive routes `matmul()` to `TSPARSE_MATMUL`, skipping multiplications against zero-weight elements entirely — **not as a software optimization, but as a first-class VM opcode**.

```
Benchmark (512×512 weight matrix, 56% sparsity):
  Dense ops:   262144
  Sparse ops:  115343
  Speedup:     2.3× fewer multiplications
```

---

## FPGA / Hardware

The `ternlang-hdl` crate emits synthesisable Verilog-2001:

- Trit primitives: `trit_neg`, `trit_cons`, `trit_mul`, `trit_add`, `trit_reg`, `bet_alu`
- Sparse matmul array: per-cell clock-gating on zero weights
- Full BET processor: register file (27×2-bit), PC, control unit, top-level wiring

```bash
ternlang sim program.tern          # emit testbench
iverilog -o sim.vvp program.sim.v  # compile
vvp sim.vvp                        # run
# open bet_sim.vcd in GTKWave
```

---

## MCP Integration

Any AI agent with MCP support becomes a ternary decision engine:

```json
{
  "mcpServers": {
    "ternlang": {
      "command": "/path/to/ternlang-mcp"
    }
  }
}
```

Tools available via MCP: `trit_decide`, `trit_consensus`, `trit_eval`, `ternlang_run`, `quantize_weights`, `sparse_benchmark`.

---

## The Ecosystem

Ternlang is designed to be the **convergence point** for ternary computing work happening across the field. See [TERNARY-ECOSYSTEM.md](TERNARY-ECOSYSTEM.md) for how existing projects relate to and can interoperate with ternlang.

---

## Roadmap Highlights

- [x] BET VM + full ISA (opcodes 0x00–0x32)
- [x] Lexer / Parser / Semantic checker / Codegen
- [x] `@sparseskip` → `TSPARSE_MATMUL` (flagship feature)
- [x] BitNet ternary ML kernels
- [x] Actor model: `agent` / `spawn` / `send` / `await`
- [x] Distributed runtime (TCP, `TernNode`)
- [x] Verilog-2001 HDL backend + FPGA simulation wrapper
- [x] LSP 3.17 language server
- [x] VS Code extension (syntax + LSP)
- [x] MCP server
- [x] Package manager (`ternpkg`)
- [ ] VS Code Marketplace publication
- [ ] crates.io publication (`ternlang-trit`)
- [ ] Academic whitepaper (BET ISA + sparse inference)
- [ ] Ecosystem bridges (9-trit, Owlet, T-CPU)

Full roadmap: [ROADMAP.md](ROADMAP.md)

---

## License

LGPL-3.0 — compiler and stdlib contributions flow back to the commons.
Commercial licensing for ML kernels, HDL backend, and distributed runtime: contact RFI-IRFOS.

**Built by Simeon Kepp & Claude — RFI-IRFOS, 2026**
