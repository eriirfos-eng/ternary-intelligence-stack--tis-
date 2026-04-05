# Ternary Intelligence Stack

**A balanced ternary language, virtual machine, and AI reasoning platform.**

[![crates.io](https://img.shields.io/crates/v/ternlang-core.svg)](https://crates.io/crates/ternlang-core)
[![License](https://img.shields.io/badge/license-LGPL--3.0%20%2F%20BSL--1.1-blue)](ternlang-root/LICENSE)
[![Tests](https://img.shields.io/badge/tests-146%2B%20passing-brightgreen)](ternlang-root/ROADMAP.md)
[![API](https://img.shields.io/badge/API-live-brightgreen)](https://ternlang-api.fly.dev/health)
[![MCP](https://img.shields.io/badge/MCP-10%20tools-purple)](https://ternlang.com/mcp)

Built by [RFI-IRFOS](https://ternlang.com) · [ternlang.com](https://ternlang.com)

---

Binary systems treat uncertainty as null. Ternlang treats it as a **state**.

Every value in ternlang is a *trit* — one of three:

```
-1  →  reject    Clear negative signal. Do not proceed.
 0  →  hold      Not enough data. Gather more before acting.
+1  →  affirm    Clear positive signal. Proceed.
```

The `hold` state is the core innovation. It is not null. It is not undecided. It is a **computational instruction** — a formal signal that tells the system to remain in deliberation until evidence is sufficient. This makes ternlang the natural foundation for AI agents that must reason honestly under uncertainty, sparse neural inference where zero-weights are skipped at the instruction level, and safety-critical systems where a premature decision is worse than no decision.

---

## What's in This Repository

```
ternlang-root/        Language, VM, inference engine, API, MCP server
albert-agent/         Local AI node built on the Ternary Intelligence Stack
ternlang-vscode/      VS Code extension (.tern syntax highlighting + LSP)
```

→ **[Full technical documentation](ternlang-root/README.md)**
→ **[Development roadmap](ternlang-root/ROADMAP.md)**
→ **[250+ .tern example programs](ternlang-root/examples/INDEX.md)**

---

## The Stack at a Glance

| Layer | What it does |
|-------|-------------|
| **Language** | `.tern` programs compile to BET bytecode and run on the BET VM — 51 opcodes, 27 registers, exhaustive 3-way match enforcement |
| **Sparse Inference** | `@sparseskip` routes `matmul()` to `TSPARSE_MATMUL` — zero-weight elements skipped at the instruction level. **86–122× faster** than dense float32 at BitNet sparsity levels |
| **MoE-13 Orchestrator** | Mixture-of-Experts reasoning engine: 13 domain experts, dual-key synergistic routing, 1+1=3 emergent triad synthesis, safety hard gate with permanent audit log |
| **Reasoning Toolkit** | Deliberation engine (EMA convergence), coalition vote, action gate (hard-block safety veto), scalar temperature, hallucination score |
| **Live API** | REST + SSE + MCP endpoints at `https://ternlang.com` — deployed on Fly.io |
| **MCP Server** | 10 tools via HTTP or stdio — any MCP client becomes a ternary decision engine |

---

## MoE-13 Ternary Orchestrator

The flagship reasoning component. Based on prior research ([DOI: 10.17605/OSF.IO/TZ7DC](https://doi.org/10.17605/OSF.IO/TZ7DC)).

```rust
use ternlang_moe::TernMoeOrchestrator;

let mut orch = TernMoeOrchestrator::with_standard_experts();
let result = orch.orchestrate("Should I proceed?", &[0.6, 0.7, 0.8, 0.5, 0.4, 0.9]);

// trit=1 conf=84% held=false
// "Affirm with confidence 84%. Emergent field amplifying."
```

Routes through 13 specialists: Syntax · WorldKnowledge · DeductiveReason · InductiveReason · ToolUse · Persona · Safety · FactCheck · CausalReason · AmbiguityRes · MathReason · ContextMem · MetaSafety.

Safety is a hard gate — a negative safety signal vetoes the entire result regardless of all other experts, and every veto is permanently logged.

---

## Live API

```bash
# Health check
curl https://ternlang.com/health

# MCP — no API key required
curl -X POST https://ternlang.com/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call",
       "params":{"name":"trit_decide","arguments":{"evidence":[0.8,0.6,-0.2,0.9]}}}'

# REST — requires X-Ternlang-Key
curl -X POST https://ternlang.com/api/moe/orchestrate \
  -H "X-Ternlang-Key: your_key" \
  -H "Content-Type: application/json" \
  -d '{"query":"Is this action safe?"}'
```

**MCP server:** `https://ternlang.com/mcp` — compatible with Claude Desktop, Smithery, and any HTTP MCP client.

```json
{ "mcpServers": { "ternlang": { "url": "https://ternlang.com/mcp" } } }
```

---

## Sparse Inference Benchmark

| Sparsity | 128² | 256² | 512² |
|----------|------|------|------|
| 40% | 29.6× | 46.0× | 73.6× |
| **60%** | **27.9×** | **32.1×** | **86.1×** |
| 99% | 13.1× | 53.9× | **122.3×** |

40–60% sparsity is exactly where BitNet b1.58 quantization (`τ = 0.5 × mean(|w|)`) places weights in trained language models. The kernel and the quantization scheme are structurally aligned.

---

## Albert Agent

[albert-agent/](albert-agent/) is a sovereign, offline-first local AI node built on top of the Ternary Intelligence Stack. It uses the BET VM and MoE-13 orchestrator as its native reasoning layer — every decision is evaluated through the `{-1, 0, +1}` state space rather than a binary confidence threshold.

→ [Albert Agent documentation](albert-agent/README.md)

---

## Quick Start

```bash
git clone https://github.com/eriirfos-eng/ternary-intelligence-stack
cd ternary-intelligence-stack/ternlang-root
cargo build --release
cargo test --workspace

# Run a .tern program
cargo run --bin ternlang -- run examples/03_rocket_launch.tern

# Start the MCP server (stdio)
./target/release/ternlang-mcp
```

---

## Crates

All published on [crates.io](https://crates.io/search?q=ternlang):

| Crate | Tier | Description |
|-------|------|-------------|
| `ternlang-core` | LGPL | Lexer, parser, BET VM |
| `ternlang-cli` | LGPL | `run` · `build` · `sim` · `fmt` · `repl` |
| `ternlang-lsp` | LGPL | LSP 3.17 server |
| `ternlang-compat` | LGPL | 9-trit RISC assembler, Owlet S-expr bridge |
| `ternpkg` | LGPL | Package manager |
| `ternlang-ml` | BSL-1.1 | Sparse matmul, BitNet quantization, reasoning toolkit |
| `ternlang-moe` | BSL-1.1 | MoE-13 orchestrator, AgentHarness |
| `ternlang-mcp` | BSL-1.1 | MCP server — 10 tools |
| `ternlang-api` | BSL-1.1 | REST + SSE API |
| `ternlang-hdl` | BSL-1.1 | Verilog-2001 codegen, FPGA simulation |
| `ternlang-runtime` | BSL-1.1 | Distributed TCP actor runtime |

BSL-1.1 converts automatically to Apache-2.0 on 2030-04-03.

---

## Whitepaper

[ternlang-root/whitepaper/](ternlang-root/whitepaper/) — IEEE two-column, arXiv-ready.

```bibtex
@misc{kepp2026ternlang,
  author = {Kepp, Simeon},
  title  = {Ternlang: Balanced Ternary Intelligence Stack},
  year   = {2026},
  url    = {https://ternlang.com},
  doi    = {10.17605/OSF.IO/TZ7DC}
}
```

---

## License

Open core under **LGPL-3.0**. Commercial components under **BSL-1.1**.
Commercial licensing: [licensing@ternlang.com](mailto:licensing@ternlang.com)

> The contents of this repository may not be used to train, fine-tune, or distill machine learning models without explicit written permission from RFI-IRFOS.

---

*Built by Simeon Kepp · RFI-IRFOS · 2026*


<!-- Index Nudge: Sun Apr  5 20:04:33 GMT 2026 -->
