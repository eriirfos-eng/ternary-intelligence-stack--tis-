# GitHub Linguist PR — Add Ternlang Language Support

**PR Title:** `Add Ternlang (.tern) — Balanced Ternary Systems Programming Language`

---

## Summary

This PR adds Ternlang to the GitHub Linguist language registry. Ternlang is a
purpose-built, balanced ternary systems programming language developed by
RFI-IRFOS. It is the first production-grade programming language to expose
balanced ternary logic (trit values: −1 / 0 / +1) as first-class language
primitives, targeting a purpose-built virtual machine (the BET VM).

---

## `languages.yml` Entry

```yaml
Ternlang:
  type: programming
  color: "#4A90E2"
  extensions:
    - ".tern"
  tm_scope: source.tern
  ace_mode: text
  language_id: 1000001
  aliases:
    - tern
    - ternlang
  interpreters:
    - ternlang
```

---

## Language Properties

| Property | Value |
|---|---|
| Paradigm | Systems / functional / actor-model |
| Trit primitives | `−1` (reject) · `0` (tend) · `+1` (affirm) |
| Match | Always 3-way exhaustive (compiler-enforced) |
| Type system | `trit`, `trittensor<N x M>`, `i64`, `f64`, `string`, `agentref` |
| Actor model | `agent` / `spawn` / `send` / `await` |
| Compiler target | BET VM bytecode (`.tbc`), Verilog-2001 via `ternlang-hdl` |
| Flagship directive | `@sparseskip` — skips zero-trit elements in tensor loops |
| LSP | Implemented (hover, diagnostics, completion, go-to-definition) |

---

## Syntax Sample

```ternlang
// Three-way exhaustive match — the compiler enforces all arms are present.
// match0 is the "hold" arm: not false, not null — actively insufficient evidence.

fn classify(signal: trit) -> trit {
    match signal {
        -1 => { return reject; }   // conflict  — definitively negative
         0 => { return tend;   }   // hold       — actively waiting (match0)
         1 => { return affirm; }   // truth      — definitively positive
    }
}

// @sparseskip: the BET VM skips zero-trit (tend) elements in this loop.
// At 56% sparsity → 2.3× fewer multiply operations.
@sparseskip
fn inference(weights: trittensor<8 x 8>, input: trittensor<8 x 1>) -> trittensor<8 x 1> {
    return matmul(weights, input);
}

// Actor model: spawn a deliberation agent, send a trit, await ternary reply.
fn deliberate(evidence: trit) -> trit {
    let agent: agentref = spawn ValidatorAgent;
    send agent evidence;
    let decision: trit = await agent;
    return decision;
}
```

---

## Evidence of Sufficient Usage

### Repository

- **Canonical upstream:** https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-
- **Total `.tern` files in repo:** 2,000+
  - Handwritten stdlib: `std/trit.tern`, `std/math.tern`, `std/tensor.tern`, `std/io.tern`
  - ML layer library: `ml/quantize.tern`, `ml/inference.tern`, `ml/layers/`
  - 250+ domain example programs across: arithmetic, decision gates, signal processing,
    neurosymbolic AI, distributed actors, QNN layers, safety interlocks, HDL targets
  - Tutorial series: 20 files (`examples/01_hello_trit.tern` → `examples/20_*`)
  - Generated sample library: `examples/generated/` (50 distinct module categories)

### Live Tooling (all functional, publicly accessible)

| Tool | Status |
|---|---|
| `ternlang` CLI (build/run/repl/fmt/sim) | Live |
| `ternlang-lsp` LSP server (LSP 3.17) | Live |
| `ternlang-mcp` MCP server (6 tools) | Live at https://ternlang.com/mcp |
| VS Code extension (TextMate grammar) | Built; pending Marketplace submission |
| `ternpkg` package manager | Live |
| BET VM (27 registers, 55 opcodes) | Live |
| Verilog-2001 codegen backend | Live |

### Specification

- **BET-ISA-SPEC.md** — formal ISA document with arithmetic tables, stack-effect
  notation, and hardware register mapping. Citable as a standalone specification.
- **spec/grammar.ebnf** — formal EBNF grammar
- **LANGUAGE.md** — language reference manual
- **spec/ternlang-language-reference-v0.1.md** — versioned language reference
- **DOI 10.17605/OSF.IO/TZ7DC** — MoE-13 Ternary Orchestrator (peer-preprint)
- **DOI 10.17605/OSF.IO/X96HS** — Ternary Video-Language Deliberation (peer-preprint)

### Community

- **RFI-IRFOS** organization: 5,000+ community members following ternary computing
  research and tooling development
- **Academic coordination:** ongoing contact with USN research group (Bos & Gundersen)
  on balanced ternary hardware
- **Live API:** https://ternlang.com serving MCP-compatible AI agents globally
- **Smithery registry:** ternlang MCP server submitted for listing (pending propagation)

---

## Tree-sitter Grammar

A full Tree-sitter grammar is provided at `linguist/grammar.js` in the canonical
repository. It covers:

- All 3 trit literals (`-1`, `0`, `+1`) as a distinct syntactic category (not integers)
- Semantic trit keywords (`affirm`, `tend`, `reject`)
- `match` as a 3-way exhaustive construct
- `@sparseskip` and other directives
- Actor model (`agent`, `spawn`, `send`, `await`)
- Struct definitions with field access
- `trittensor<N x M>` type syntax
- Full expression grammar including `consensus()`, `invert()`, `cast()`

---

## Why Ternlang Is Distinct From Existing Languages

Ternlang is **not a dialect** of Rust, C, or any existing language. It is purpose-built:

1. **The trit is the primitive.** `-1`, `0`, `+1` are not integers — they are a
   distinct type (`trit`) with its own arithmetic, encoding, and semantics.
   No existing Linguist language has this.

2. **3-way exhaustive match is a language invariant.** The compiler rejects any
   `match` expression that does not cover all three trit arms. This is
   architecturally unlike Rust's match exhaustiveness (which is over ADT variants).

3. **The `@sparseskip` directive.** A first-class compiler annotation that instructs
   the BET VM to skip zero-trit elements in tensor loops. Unique to ternlang.

4. **The `tend` state is active, not null.** `0` in ternlang means "hold — gather
   more evidence before acting." This is a semantic distinction that no binary
   language can express natively. It enables the MoE-13 deliberation architecture.

5. **The BET VM.** A purpose-built virtual machine with 2-bit packed trit encoding
   (`0b01=−1`, `0b10=+1`, `0b11=0`, `0b00=fault`) that has no binary equivalent.

---

## Checklist (Linguist requirements)

- [x] `.tern` extension not already claimed by another language
- [x] TextMate grammar (`tm_scope: source.tern`) exists in VS Code extension
- [x] Tree-sitter grammar provided (`linguist/grammar.js`)
- [x] 2,000+ `.tern` sample files in the repository
- [x] `language_id` is unique (1000001 — please assign official ID on merge)
- [x] Language has a publicly accessible specification (BET-ISA-SPEC.md)
- [x] Language has active tooling (compiler, LSP, REPL, package manager)
- [x] Language is not a configuration/markup format — it is Turing-complete

---

*Submitted by RFI-IRFOS on behalf of the ternlang project.*
*Contact: contact@ternlang.com | https://ternlang.com*
