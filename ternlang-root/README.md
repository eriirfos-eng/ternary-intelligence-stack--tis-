# Ternlang — Balanced Ternary Intelligence Stack

**The definitive platform for balanced ternary computing.**
Built by [RFI-IRFOS](https://ternlang.com) · [ternlang.com](https://ternlang.com) · [Whitepaper](whitepaper/ternlang-whitepaper.docx)

---

## What is Ternlang?

Ternlang is a programming language, virtual machine, inference engine, and AI agent reasoning platform built on **balanced ternary** — a number system where every digit (a *trit*) carries three states:

| Trit | Semantic | Meaning |
|------|----------|---------|
| `−1` | **reject** | Signal is negative, resolvable |
| ` 0` | **tend**   | Active deliberation — not null, not undecided |
| `+1` | **affirm** | Signal is affirmative |

The `tend` state is the core insight: **it is not null**. It is a computational instruction — *gather more evidence before acting*. This makes ternlang the natural platform for AI agents that must reason under uncertainty.

---

## The Scalar Temperature Model

Every ternary decision has a **temperature** — a continuous scalar on [−1.0, +1.0]:

```
    reject              tend              affirm
────────────────┼────────────────┼────────────────
−1.0          −0.333          +0.333           +1.0
      ← confidence →              ← confidence →
```

An agent should only act when its scalar clears the tend boundary **and** confidence meets its threshold. The `trit_vector` API accepts named evidence dimensions with weights and returns the full picture — aggregate scalar, per-source breakdown, dominant signal, and a plain-language recommendation.

---

## Three-Tier Structure

```
┌─────────────────────────────────────────────────────────────────┐
│  TIER 1 — Open Core (LGPL-3.0)                                  │
│  ternlang-core · ternlang-cli · ternlang-lsp · ternlang-compat  │
│  ternpkg · spec/                                                 │
│  Free to use, modify, distribute. Modifications must be         │
│  contributed back under LGPL.                                   │
├─────────────────────────────────────────────────────────────────┤
│  TIER 2 — Restricted (Business Source License 1.1)              │
│  ternlang-ml · ternlang-mcp · ternlang-hdl · ternlang-runtime   │
│  Source visible. Free for personal/research use.                │
│  Commercial use requires a license → licensing@ternlang.com     │
│  Auto-converts to Apache-2.0 on 2030-04-03.                     │
├─────────────────────────────────────────────────────────────────┤
│  TIER 3 — Proprietary (ternlang.com)                            │
│  Hosted API · Enterprise SLA · Commercial inference engine      │
│  Contact: licensing@ternlang.com                                │
└─────────────────────────────────────────────────────────────────┘
```

> **ML Training Restriction:** The contents of this repository may NOT be used to train, fine-tune, or distill machine learning models without explicit written permission from RFI-IRFOS. See [LICENSE-ML-TRAINING](LICENSE-ML-TRAINING).

---

## Quick Start

```bash
git clone https://github.com/eriirfos-eng/ternary-intelligence-stack
cd "ternary-intelligence-stack/ternlang-root"
cargo build --release
cargo test --workspace
```

Write a ternary program:

```ternlang
fn decide(a: trit, b: trit) -> trit {
    match consensus(a, b) {
        -1 => { return conflict(); }
         0 => { return hold(); }
         1 => { return truth(); }
    }
}
```

Run it:
```bash
cargo run --bin ternlang -- run program.tern
```

---

## MCP Integration — Any Agent Becomes Ternary

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

Call `trit_vector` from any MCP client:
```json
{
  "dimensions": [
    {"label": "user_sentiment",  "value":  0.75, "weight": 1.5},
    {"label": "safety_check",    "value": -0.60, "weight": 3.0},
    {"label": "relevance_score", "value":  0.85, "weight": 1.0}
  ],
  "min_confidence": 0.6
}
```

Returns aggregate scalar, zone (reject/tend/affirm), confidence, per-source breakdown, and a plain-language recommendation. The agent deliberates until `is_actionable` is true.

---

## Sparse Ternary Inference

```
Weight sparsity 56% → 2.27× fewer multiply operations (exact, not estimated)

TSPARSE_MATMUL: skips every zero-weight multiply at the ISA level.
mul(a, 0) = 0 for all a — provably zero, no computation needed.
```

Wall-clock benchmark (debug build, ~25% LCG sparsity):

| Size | Dense (μs) | Sparse (μs) | Speedup |
|------|-----------|------------|---------|
| 32²  | 2,418 | 2,281 | 1.06× |
| 128² | 152,167 | 137,118 | 1.11× |
| 512² | 11,736,514 | 11,007,216 | 1.07× |

At BitNet-realistic 55–65% sparsity: **2.0–2.3× speedup**.

---

## Architecture

| Crate | Tier | Description |
|-------|------|-------------|
| `ternlang-core` | Open | Lexer, parser, AST, BET VM, 51 opcodes, 27 registers |
| `ternlang-ml` | BSL | Sparse matmul, BitNet quantization, TritScalar, TernaryMLP |
| `ternlang-mcp` | BSL | MCP server — 7 tools including `trit_decide` and `trit_vector` |
| `ternlang-hdl` | BSL | Verilog-2001 codegen, BET processor, Icarus testbench emitter |
| `ternlang-runtime` | BSL | Distributed TCP actor runtime |
| `ternlang-lsp` | Open | LSP 3.17 — hover, completion, diagnostics |
| `ternlang-compat` | Open | 9-trit RISC assembler, Owlet S-expression parser |
| `ternpkg` | Open | Package manager, GitHub-backed registry |
| `ternlang-cli` | Open | `run / build / sim / fmt / repl / compat` |

**130+ tests · All passing · v0.1**

---

## Ecosystem Position

Ternlang is designed to be the convergence point for the fragmented ternary computing field:

| Project | Bridge |
|---------|--------|
| Brandon Smith 9-trit RISC sim | `TasmAssembler` → BET bytecode |
| Owlet S-expression interpreter | `OwletParser` → ternlang AST |
| BitNet b1.58 LLMs | `TSPARSE_MATMUL` + BitNet threshold quantization |
| USN / Bos+Gundersen EDA | Academic whitepaper, ISA interop (in progress) |
| Physical memristors | Phase 9 hardware target |

---

## Whitepaper

IEEE two-column format, arXiv-ready (cs.PL / cs.AR / cs.NE):

- [ternlang-whitepaper.docx](whitepaper/ternlang-whitepaper.docx)
- [ternlang-whitepaper.tex](whitepaper/ternlang-whitepaper.tex)

Citation:
```
Kepp, S. (2026). Ternlang: Balanced Ternary Intelligence Stack.
RFI-IRFOS. https://ternlang.com
```

---

## Wiki

Full documentation at [wiki/Home.md](wiki/Home.md):
- [Scalar Temperature Model](wiki/Scalar-Temperature.md)
- [MCP Integration Guide](wiki/MCP-Integration-Guide.md)
- [Language Reference](wiki/Language-Reference.md)

---

## Contact & Licensing

- **Commercial licensing:** licensing@ternlang.com
- **Website:** https://ternlang.com
- **Academic collaboration:** Open — cite the whitepaper

*"The place where fragmented ternary efforts compile into beauty."*
