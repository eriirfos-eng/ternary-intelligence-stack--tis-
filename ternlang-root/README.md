# Ternlang — Balanced Ternary Intelligence Stack

**The definitive platform for balanced ternary computing.**

[![crates.io](https://img.shields.io/crates/v/ternlang-core.svg)](https://crates.io/crates/ternlang-core)
[![license](https://img.shields.io/badge/license-LGPL--3.0%20%2F%20BSL--1.1-blue)](LICENSE)
[![tests](https://img.shields.io/badge/tests-177%2B%20passing-brightgreen)](#architecture)
[![API](https://img.shields.io/badge/API-live-brightgreen)](https://ternlang-api.fly.dev/health)
[![MCP](https://img.shields.io/badge/MCP-10%20tools-purple)](https://ternlang.com/mcp)

Built by [RFI-IRFOS](https://ternlang.com) · [ternlang.com](https://ternlang.com) · [Whitepaper (DOI)](https://doi.org/10.17605/OSF.IO/TZ7DC)

---

## The Problem with Binary AI

Every AI system today is forced to answer yes or no — even when the evidence is contradictory, incomplete, or genuinely uncertain. Binary logic has no formal representation for *"I don't know yet."* Systems either hallucinate a confident answer or return null.

Ternlang adds the third state.

| Trit | Name | What it means |
|------|------|---------------|
| `−1` | **reject** | Clear negative signal. Do not proceed. |
| ` 0` | **tend** | Insufficient data. Gather more before acting. |
| `+1` | **affirm** | Clear positive signal. Proceed. |

The `tend` state is not indecision. It is a **first-class routing instruction** — a computational directive to remain in deliberation until evidence crosses a threshold. This makes ternlang the natural foundation for AI agents that must reason honestly under uncertainty.

---

## What's in This Repository

| Layer | What it does |
|-------|-------------|
| [Language & VM](#language--vm) | Compile and run `.tern` programs on the Balanced Ternary Execution VM |
| [Sparse Inference](#sparse-ternary-inference) | BitNet-style ternary weights with 86–122× speedup over dense float32 |
| [MoE-13 Orchestrator](#moe-13-ternary-orchestrator) | Mixture-of-Experts reasoning engine with safety hard gate |
| [Live API](#live-api) | REST + SSE + MCP endpoints at `https://ternlang.com` |
| [Example Library](#example-library) | 300+ `.tern` programs across every domain |
| [Ecosystem Bridges](#ecosystem-position) | Interop with Brandon Smith 9-trit, Owlet, BitNet b1.58 |

---

## Language & VM

Ternlang programs use `trit` as the only scalar type. Every `match` must cover all three arms — the compiler rejects non-exhaustive matches.

```ternlang
// A ternary medical triage gate
fn patient_conscious(signal: trit) -> trit {
    match signal {
        reject => { return reject; }   // hard gate — unconscious patient blocks all other evaluation
        tend   => { return tend;   }
        affirm => { return affirm; }
    }
}

fn vital_signs(heart: trit, pressure: trit) -> trit {
    return consensus(heart, pressure);
}

let conscious: trit = patient_conscious(affirm);

match conscious {
    reject => { return reject; }   // immediate escalation, no further checks
    tend   => { return tend;   }
    affirm => {
        let vitals: trit = vital_signs(affirm, tend);
        match vitals {
            reject => { return reject; }
            tend   => { return tend;   }
            affirm => { return affirm; }
        }
    }
}
```

**Built-in functions:** `consensus(a, b)` · `invert(x)` · `truth()` · `hold()` · `conflict()`

**Quick start — install the CLI:**

```bash
cargo install ternlang-cli
```

Then run any `.tern` file directly from your terminal:

```bash
ternlang run my_program.tern
ternlang run examples/03_rocket_launch.tern
ternlang build my_program.tern --output my_program.bet
ternlang repl
ternlang fmt my_program.tern --write
```

**Or build from source:**

```bash
git clone https://github.com/eriirfos-eng/ternary-intelligence-stack
cd ternary-intelligence-stack/ternlang-root
cargo build --release
./target/release/ternlang run examples/03_rocket_launch.tern
```

---

## Sparse Ternary Inference

`mul(a, 0) = 0` for all `a` — provably zero, no computation needed. The `ternlang-ml` kernel precomputes a Compressed Sparse Column index, flattens weights to raw `i8`, and dispatches rows in parallel via Rayon. No branches in the inner loop.

**Goldilocks sparsity sweep** (release build, 3-rep median):

| Sparsity | 32² | 64² | 128² | 256² | 512² |
|----------|-----|-----|------|------|------|
| 25% | 6.3× | 11.5× | 26.4× | 39.3× | 53.1× |
| 40% | 6.3× | 13.1× | 29.6× | 46.0× | 73.6× |
| **50%** | **5.9×** | **10.2×** | **28.7×** | **56.6×** | **82.1×** |
| **60%** | **5.8×** | **9.5×** | **27.9×** | **32.1×** | **86.1×** |
| 99% | 1.8× | 9.9× | 13.1× | 53.9× | **122.3×** |

**Peak: 122× at 512×512, 99% sparsity.**
**Goldilocks zone: 40–60% → 20–86× on medium matrices.** This is exactly where BitNet b1.58 quantization (`τ = 0.5 × mean(|w|)`) naturally places weights in trained language models. The kernel and the quantization scheme are structurally aligned.

---

## MoE-13 Ternary Orchestrator

`ternlang-moe` implements the MoE-13 architecture ([DOI: 10.17605/OSF.IO/TZ7DC](https://doi.org/10.17605/OSF.IO/TZ7DC)) — a ternary Mixture-of-Experts system that routes queries through a pool of 13 domain experts, synthesises an emergent signal, enforces a hard safety veto, and returns a ternary decision with confidence and temperature.

```rust
use ternlang_moe::TernMoeOrchestrator;

let mut orch = TernMoeOrchestrator::with_standard_experts();

// [syntax, world_knowledge, reasoning, tool_use, persona, safety]
let evidence = [0.6, 0.7, 0.8, 0.5, 0.4, 0.9];
let result = orch.orchestrate("Should I proceed with this action?", &evidence);

println!("trit={} conf={:.0}% held={}", result.trit, result.confidence * 100.0, result.held);
// → trit=1 conf=84% held=false
println!("{}", result.prompt_hint);
// → "Affirm with confidence 84%. Emergent field amplifying."
```

**How it works:**

1. **Dual-key routing** — scores every expert pair by `relevance_a × relevance_b × synergy`. Complementary experts outperform redundant ones.
2. **1+1=3 triad synthesis** — emergent field `Ek = synergy × (vi + vj) / 2`. Two orthogonal experts produce a third signal neither could generate alone.
3. **Safety hard gate** — Axis-6 veto fires before any vote. Every veto is permanently logged to `AxisMemory` for audit.
4. **Hold with tiebreaker** — a split vote or low confidence yields `trit=0`. The orchestrator invokes a tiebreaker (max 4 active experts) before committing, modelling the human *"let me think about this"* behaviour.
5. **Three-tier memory** — Node (TTL: seconds), Cluster (routing frequency, mode-collapse risk), Axis (persistent priors + veto audit log).

**13 standard experts:** Syntax · WorldKnowledge · DeductiveReason · InductiveReason · ToolUse · Persona · Safety · FactCheck · CausalReason · AmbiguityRes · MathReason · ContextMem · MetaSafety

**AgentHarness** provides a pluggable interface for all 13 experts:

```rust
use ternlang_moe::agents::AgentHarness;

let harness = AgentHarness::with_standard_agents();
let verdicts = harness.run("Is this safe to execute?", &evidence);
```

---

## Live API

The full TIS API runs at **`https://ternlang.com`** — deployed on Fly.io, Frankfurt region.

```bash
# Health check
curl https://ternlang.com/health

# MoE-13 orchestration (no API key required for MCP)
curl -X POST https://ternlang.com/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call",
       "params":{"name":"moe_orchestrate",
                 "arguments":{"query":"Should I send this email?"}}}'

# Scalar ternary decision (API key required)
curl -X POST https://ternlang.com/api/trit_decide \
  -H "X-Ternlang-Key: your_key" \
  -H "Content-Type: application/json" \
  -d '{"evidence":[0.8, -0.2, 0.6, 0.9]}'
```

**REST endpoints** (require `X-Ternlang-Key`):

| Endpoint | Description |
|----------|-------------|
| `POST /api/trit_decide` | Float evidence array → reject / tend / affirm + confidence |
| `POST /api/trit_vector` | Named dimensions with weights → aggregate ternary decision |
| `POST /api/trit_consensus` | `consensus(a, b)` → ternary result |
| `POST /api/trit_deliberate` | EMA convergence loop — multi-round evidence → stable trit |
| `POST /api/trit_coalition` | N-agent weighted vote → quorum / dissent / abstain |
| `POST /api/trit_gate` | Multi-dimensional hard-block safety gate |
| `POST /api/moe/orchestrate` | Full MoE-13 pass — synchronous JSON result |
| `GET  /api/stream/moe_orchestrate` | MoE-13 pass streamed round-by-round via SSE |
| `GET  /api/stream/deliberate` | EMA deliberation streamed per round via SSE |
| `GET  /api/usage` | Monthly usage stats for the authenticated key |

**API key:** [ternlang.com/pricing](https://ternlang.com/pricing) · Tier 2 (€24/month): 10,000 calls/month, calendar-month reset

### MCP Server

The MCP server runs at `https://ternlang.com/mcp` — compatible with Claude Desktop, Smithery, and any HTTP MCP client.

**10 tools:** `trit_decide` · `trit_consensus` · `trit_eval` · `ternlang_run` · `quantize_weights` · `sparse_benchmark` · `moe_orchestrate` · `moe_deliberate` · `trit_action_gate` · `trit_enlighten`

```json
{
  "mcpServers": {
    "ternlang": {
      "url": "https://ternlang.com/mcp"
    }
  }
}
```

For local stdio transport (Claude Desktop, offline use):
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

---

## Example Library

**300+ `.tern` programs** covering real-world decision logic across every domain — the largest collection of balanced ternary programs in existence.

| Category | Examples |
|----------|---------|
| [Aerospace & Safety](examples/03_rocket_launch.tern) | Rocket launch, aircraft deicing, runway incursion, satellite collision |
| [Medicine](examples/05_medical_triage.tern) | ER triage, ICU ventilator, sepsis warning, organ transplant, APGAR |
| [Finance](examples/42_algorithmic_trading.tern) | Algorithmic trading, AML filter, options expiry, loan underwriting |
| [Infrastructure](examples/14_circuit_breaker.tern) | Circuit breaker, nuclear reactor SCRAM, bridge health, power grid |
| [AI Agents](examples/08_evidence_collector.tern) | Evidence density, confidence escalation, MoE routing, deliberation |
| [Civic Systems](examples/12_vote_aggregator.tern) | Vote aggregation, bail decision, treaty negotiation, refugee status |
| [Computer Science](examples/09_risc_fetch_decode.tern) | CPU pipeline, cache invalidation, API rate limiting, deployment gate |
| [Tutorials](stdlib/tutorials/) | 15 step-by-step tutorials — hello ternary → full ML pipeline |
| [QNN / Qutrit](stdlib/qnn/) | Qutrit Neural Networks — Kepp 2026 reference implementations |
| [Standard Library](stdlib/) | Agents, reasoning, ML layers, optimizers, std, benchmarks |

→ [**Browse all examples**](examples/INDEX.md)

---

## Architecture

| Crate | Tier | Description |
|-------|------|-------------|
| [`ternlang-core`](ternlang-core/) | Open (LGPL) | Lexer, parser, AST, BET VM — 51 opcodes, 27 registers |
| [`ternlang-cli`](ternlang-cli/) | Open (LGPL) | `run` · `build` · `sim` · `fmt` · `repl` · `compat` |
| [`ternlang-lsp`](ternlang-lsp/) | Open (LGPL) | LSP 3.17 — hover, completion, diagnostics |
| [`ternlang-compat`](ternlang-compat/) | Open (LGPL) | 9-trit RISC assembler (Brandon Smith bridge), Owlet S-expr parser |
| [`ternpkg`](ternpkg/) | Open (LGPL) | Package manager, GitHub-backed registry |
| [`ternlang-ml`](ternlang-ml/) | BSL-1.1 | Sparse matmul, BitNet quantization, TernaryMLP, deliberation engine, coalition vote, action gate |
| [`ternlang-moe`](ternlang-moe/) | BSL-1.1 | MoE-13 orchestrator — dual-key routing, triad synthesis, 3-tier memory, AgentHarness |
| [`ternlang-api`](ternlang-api/) | BSL-1.1 | REST + SSE API, multi-tenant key management, all reasoning endpoints |
| [`ternlang-mcp`](ternlang-mcp/) | BSL-1.1 | MCP server — 10 tools, stdio + HTTP transport |
| [`ternlang-hdl`](ternlang-hdl/) | BSL-1.1 | Verilog-2001 codegen, BET processor, FPGA simulation |
| [`ternlang-runtime`](ternlang-runtime/) | BSL-1.1 | Distributed TCP actor runtime |

**146+ tests · All passing · v0.1.0**

---

## Licensing Tiers

```
┌─────────────────────────────────────────────────────────────────┐
│  TIER 1 — Open Core (LGPL-3.0)                                  │
│  ternlang-core · ternlang-cli · ternlang-lsp · ternlang-compat  │
│  ternpkg · spec/                                                 │
│  Free to use, modify, and distribute. Modifications must be     │
│  contributed back under LGPL.                                   │
├─────────────────────────────────────────────────────────────────┤
│  TIER 2 — Restricted (Business Source License 1.1)              │
│  ternlang-ml · ternlang-mcp · ternlang-hdl · ternlang-runtime   │
│  ternlang-moe · ternlang-api                                    │
│  Source visible. Free for personal and research use.            │
│  Commercial use requires a license → licensing@ternlang.com     │
│  Auto-converts to Apache-2.0 on 2030-04-03.                     │
├─────────────────────────────────────────────────────────────────┤
│  TIER 3 — Proprietary (ternlang.com)                            │
│  Hosted API · Enterprise SLA · Commercial inference engine      │
│  Contact: licensing@ternlang.com                                │
└─────────────────────────────────────────────────────────────────┘
```

> **ML Training Restriction:** The contents of this repository may not be used to train, fine-tune, or distill machine learning models without explicit written permission from RFI-IRFOS. See [LICENSE-ML-TRAINING](LICENSE-ML-TRAINING).

---

## Ecosystem Position

Ternlang is designed to be the convergence point for the fragmented ternary computing field.

| Project | Bridge |
|---------|--------|
| [Brandon Smith 9-trit RISC simulator](https://github.com/brandon-smith-187) | `TasmAssembler` in `ternlang-compat` — assembles `.tasm` → BET bytecode |
| [Owlet S-expression interpreter](https://github.com/owlet-lang) | `OwletParser` in `ternlang-compat` — S-expr front-end → ternlang AST |
| [BitNet b1.58](https://arxiv.org/abs/2402.17764) | `TSPARSE_MATMUL` + `bitnet_threshold()` — structurally aligned quantization |
| USN / Bos & Gundersen (EDA ternary logic) | Academic whitepaper — ISA interop in progress |
| Physical memristor arrays | Phase 10 hardware target |

→ [**Full ecosystem map**](TERNARY-ECOSYSTEM.md)

---

## Whitepaper & Specs

- [ternlang-whitepaper.tex](whitepaper/ternlang-whitepaper.tex) — IEEE two-column, arXiv-ready (cs.PL / cs.AR / cs.NE)
- [BET-ISA-SPEC.md](BET-ISA-SPEC.md) — formal ISA specification with encoding tables and stack-effect notation
- [spec/grammar.ebnf](spec/grammar.ebnf) — language grammar
- [spec/ternlang-language-reference-v0.1.md](spec/ternlang-language-reference-v0.1.md) — language reference

```bibtex
@misc{kepp2026ternlang,
  author  = {Kepp, Simeon},
  title   = {Ternlang: Balanced Ternary Intelligence Stack},
  year    = {2026},
  url     = {https://ternlang.com},
  doi     = {10.17605/OSF.IO/TZ7DC}
}
```

---

## Contact & Licensing

| | |
|---|---|
| **Website** | [ternlang.com](https://ternlang.com) |
| **Commercial licensing** | [licensing@ternlang.com](mailto:licensing@ternlang.com) |
| **Academic collaboration** | Open — cite the whitepaper |
| **API access** | [ternlang.com/#licensing](https://ternlang.com/#licensing) |

*"The place where fragmented ternary efforts compile into one."*
