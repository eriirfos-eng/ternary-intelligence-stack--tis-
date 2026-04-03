# Welcome to the Ternary Intelligence Stack Wiki

**Ternlang / RFI-IRFOS** — the definitive hub for balanced ternary computing.

This wiki is the living documentation for the project: roadmap, architecture, API reference, and guides for contributors and integrators.

---

## What is Ternlang?

Ternlang is the first complete software stack for **balanced ternary computing** — a number system where every digit (a *trit*) carries three states instead of two:

| Trit | Value | Semantic | Meaning |
|------|-------|----------|---------|
| `-1` | NegOne | **reject** | Signal is negative, resolvable |
| ` 0` | Zero   | **tend**   | Active deliberation — not null, not undecided |
| `+1` | PosOne | **affirm** | Signal is affirmative |

The `tend` state is the most important innovation: **it is not null**. It is an active computational instruction — *keep gathering evidence before acting*. This makes ternlang the natural language for AI agents that must reason under uncertainty.

---

## Why Ternary?

Three reasons it matters right now:

1. **AI inference** — BitNet-quantized LLMs have weights in {−1, 0, +1} with 55–65% zero elements. Ternlang's `TSPARSE_MATMUL` opcode skips all zero-weight multiplications — no approximation, just architectural truth.

2. **Ambiguity-aware agents** — binary agents collapse uncertainty into YES/NO. A ternary agent can hold, deliberate, and act only when its evidence scalar clears the tend boundary.

3. **Hardware** — ternary logic is area-efficient: 1 trit ≈ 1.58 bits. A balanced ternary ALU negates by wire-swap — zero gates. The BET VM generates synthesisable Verilog-2001.

---

## Quick Start

```bash
git clone https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-.git
cd "Ternary Intelligence Stack (TIS)/ternlang-root"
cargo build --release
cargo test --workspace
```

Run a ternary program:
```bash
cargo run --bin ternlang -- run examples/hello.tern
```

Connect an AI agent via MCP:
```bash
cargo run --release --bin ternlang-mcp
# then call trit_decide or trit_vector from any MCP client
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    ternlang source (.tern)               │
└──────────────────────┬──────────────────────────────────┘
                       │ ternlang-core
         ┌─────────────▼──────────────┐
         │  Lexer → Parser → AST      │
         │  Semantic checker          │
         │  BET Bytecode Emitter      │
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │     BET VM (27 registers)  │
         │     2-bit trit encoding    │
         │     51 opcodes             │
         └──────┬──────────┬──────────┘
                │          │
    ┌───────────▼──┐  ┌───▼────────────┐
    │ ternlang-ml  │  │ ternlang-hdl   │
    │ sparse matmul│  │ Verilog codegen│
    │ TernaryMLP   │  │ BET processor  │
    │ TritScalar   │  │ FPGA targets   │
    └──────────────┘  └────────────────┘
```

| Crate | Role |
|-------|------|
| `ternlang-core` | Lexer, parser, AST, VM, BET bytecode |
| `ternlang-ml` | BitNet quantization, sparse matmul, TritScalar, TritEvidenceVec |
| `ternlang-hdl` | Verilog-2001 codegen, BET processor, Icarus Verilog testbench emitter |
| `ternlang-mcp` | MCP server — 7 tools for AI agent integration |
| `ternlang-lsp` | LSP 3.17 server (hover, completion, diagnostics) |
| `ternlang-runtime` | Distributed TCP actor runtime |
| `ternlang-compat` | 9-trit RISC assembler, Owlet S-expression parser |
| `ternpkg` | Package manager with GitHub-backed registry |
| `ternlang-cli` | `ternlang run / build / sim / fmt / repl` |

---

## The Scalar Temperature Model

Every ternary decision has a **temperature** — a continuous scalar on [−1.0, +1.0] that tells you not just *which* trit zone you're in, but *how deeply*:

```
reject  ────────────|────────────── tend ──────────────|──────────────── affirm
−1.0             −0.333                              +0.333              +1.0
          ← confidence grows →                ← confidence grows →
```

An AI agent should act only when `is_actionable(min_confidence)` returns true:
- Zone is `reject` or `affirm` (not `tend`)
- Confidence ≥ your threshold

For multi-source reasoning, use **TritEvidenceVec** — named dimensions with weights that aggregate to a single scalar. The MCP `trit_vector` tool exposes this directly to any agent.

---

## MCP Integration (AI Agents)

Add to your agent's MCP config (`mcp-config.json`):

```json
{
  "mcpServers": {
    "ternlang": {
      "command": "/path/to/ternlang-mcp",
      "args": []
    }
  }
}
```

**Available tools:**

| Tool | Description |
|------|-------------|
| `trit_decide` | evidence[] → scalar + reject/tend/affirm + confidence |
| `trit_vector` | named dimensions + weights → aggregate + breakdown |
| `trit_consensus` | consensus(a, b) → ternary result |
| `trit_eval` | evaluate ternlang expression on BET VM |
| `ternlang_run` | compile + run full .tern program |
| `quantize_weights` | f32[] → ternary weights via BitNet threshold |
| `sparse_benchmark` | sparse vs dense matmul stats |

---

## Current Status

**v0.1 — 2026-04-03 — 130+ tests passing**

- ✅ Phase 1–2: Core language, VM, CLI
- ✅ Phase 3: Sparse inference (TSPARSE_MATMUL, TCOMPRESS/TUNPACK, TernaryMLP)
- ✅ Phase 3.5: MCP server (7 tools, scalar temperature model)
- ✅ Phase 4: Language completeness (for/while/loop/structs/match exhaustiveness)
- ✅ Phase 5: Actor model + distributed runtime (TCP)
- ✅ Phase 6: Hardware backend (Verilog-2001, BET processor, FPGA sim)
- ✅ Phase 7A: Ecosystem bridges (tasm assembler, Owlet parser, VS Code VSIX)
- 🔄 Phase 7B: crates.io + VS Code Marketplace publication
- 🔜 Phase 8: Training loop (BitNet gradient quantization)
- 🔜 Phase 9: FPGA synthesis (Artix-7 / Lattice ECP5)

Full roadmap: [ROADMAP.md](../ternlang-root/ROADMAP.md)

---

## Wiki Pages

- [[Architecture]] — deep dive into BET VM, ISA, and crate structure
- [[Language Reference]] — ternlang syntax, types, operators, match
- [[Scalar Temperature]] — TritScalar, TritEvidenceVec, confidence model
- [[MCP Integration Guide]] — connecting AI agents
- [[Hardware Backend]] — Verilog codegen and FPGA targets
- [[Ecosystem Bridges]] — tasm, Owlet, BitNet compatibility
- [[Contributing]] — how to add opcodes, extend the grammar, write tests
- [[Whitepaper]] — academic paper (LaTeX + DOCX)

---

*Built by RFI-IRFOS. Full resource commitment.*
*"The place where fragmented ternary efforts compile into beauty."*
