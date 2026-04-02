# Ternlang Roadmap: Bridging the Ternary Software Deficit
### Project: Ternary Intelligence Stack (TIS) | RFI-IRFOS
**Current Version:** v0.1 (Foundational)
**Last Updated:** 2026-04-02
**Repo:** https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-
**Local:** ~/Desktop/Ternary Intelligence Stack (TIS)/

---

## 🎯 Strategic Objective
Position RFI-IRFOS as the definitive middleware provider for ternary computing by commercializing **ternlang** as the standard paradigm for ambiguity-aware AI agents and sparse inference.

This is our philosopher's stone. Full resource commitment.

---

## 🔁 Development Protocol (READ THIS FIRST)
- **Always work locally AND push to GitHub** after every meaningful session
- Push command (credentials stored): `git push origin main` from inside `Ternary Intelligence Stack (TIS)/`
- Pull before starting a new session: `git pull origin main`
- Update this ROADMAP at the end of every session with current status
- The AI assistant (Claude) maintains a memory file at `~/.claude/projects/.../memory/project_ternlang.md` — update it each session too

---

## ✅ Phase 1: Core Language & VM Stability — COMPLETE
- [x] **Trit Primitives**: `-1`, `0`, `+1` logic (Sum/Carry, Neg, Mul) — fully tested
- [x] **Lexer**: Tokenize ternary-specific keywords (`trit`, `trittensor`, `?`, `sparseskip`)
- [x] **Skeletal Parser**: Parse basic expressions and `IfTernary` (`if ?`)
- [x] **BET VM Core**: Stack, 27 registers, carry reg, 2-bit packing (`0b01=-1`, `0b10=+1`, `0b11=0`)
- [x] **Parser Completion**: `Function`, `Program`, `match` with 3-way exhaustive branching
- [x] **Codegen (Bytecode Emitter)**: Jump resolution, register allocation, symbol table
- [x] **VM Enhancements**: Carry handling in `Tadd`, rich `VmError` reporting

---

## ✅ Phase 2: Standard Library & CLI Integration — COMPLETE
- [x] **CLI Driver**: `ternlang run <file>` and `ternlang build <file>` (clap-based)
- [x] **Built-in Functions**: `consensus(a,b)`, `invert(x)`, `truth()`, `hold()`, `conflict()`
- [ ] **Standard Library (`std::trit`)**: Initial module structure — NOT STARTED

---

## 🔴 Known Bugs (Fix Before Phase 3 Progress)
- [x] **`DimSeparator` vs `Ident` collision**: FIXED — removed dedicated token, `x` now correctly tokenizes as `Ident` everywhere
- [ ] **No function call dispatch**: Functions emit inline — no `TCALL`/`TRET` opcodes, no call stack
- [ ] **`@sparseskip` is a stub**: Parsed correctly but codegen ignores it (TODO comment in `betbc.rs`)
- [ ] **Semantic checker mocks all `Call` return types as `Trit`** — needs real function table lookup
- [ ] **Match exhaustiveness not enforced** in semantic checker — compiler should reject non-exhaustive 3-way match

---

## 🛠 Phase 3: TritTensors & Sparse Inference — IN PROGRESS
**This is the commercial differentiator. The AI inference story.**

- [x] **Fix `DimSeparator` bug** in lexer (remove dedicated token, handle as `Ident("x")` in type parser) — 11/11 tests passing
- [x] **TCALL/TRET opcodes**: Real function call dispatch with call stack — DONE
- [x] **TCALL/TRET opcodes**: Real function call dispatch with call stack — DONE
- [x] **TritTensor VM Operations** — DONE (14/14 tests passing):
    - [x] `0x20` `TMATMUL` — multiply two tensor refs
    - [x] `0x21` `TSPARSE_MATMUL` — matmul skipping zero-state weights (flagship) ⭐
    - [x] `0x22` `TIDX` — index into tensor (tensor_ref, row, col → trit)
    - [x] `0x23` `TSET` — store trit at tensor index
    - [x] `0x24` `TSHAPE` — push tensor dimensions to stack
    - [x] `0x25` `TSPARSITY` — compute zero-element count
- [x] **Implement `@sparseskip`** in codegen → emits `TSPARSE_MATMUL` — DONE
- [ ] **`TCOMPRESS` / `TUNPACK`** — run-length compression (next)
- [x] **Fill `ternlang-ml`** with real kernels — DONE:
    - [x] `quantize(f32_weights, threshold) -> Vec<Trit>` — BitNet-style ternary quantization
    - [x] `bitnet_threshold(weights)` — auto-compute τ = 0.5 × mean(|w|)
    - [x] `dense_matmul(a, b) -> TritMatrix` — baseline
    - [x] `sparse_matmul(a, b) -> (TritMatrix, skipped_count)` — flagship kernel
    - [x] `linear(input, W) -> (TritMatrix, skipped)` — BitNet-style ternary linear layer
    - [x] `benchmark(a, b) -> BenchmarkResult` — prints summary with skip rate
- [x] **First benchmark result**: 56% weight sparsity → **2.3x fewer multiply ops** vs dense
- [ ] **Publish sparse matmul benchmark** — write blog post / README section comparing vs float32

---

## 🧩 Phase 4: Language Completeness — IN PROGRESS
- [x] **Lexer**: `for`, `in`, `while`, `loop`, `break`, `continue`, `mut`, `use`, `module`, `pub`, `struct`, `::`, `!=`, `&&`, `||`
- [x] **AST**: `ForIn`, `WhileTernary`, `Loop`, `Break`, `Continue`, `Use` nodes; `BinOp::NotEqual/And/Or`; `Type::Bool/Float/String`
- [x] **Parser**: `for x in expr { }`, `while cond ? { } else { } else { }`, `loop { }`, `break`, `continue`, `use std::trit;`, `let mut`
- [x] **Match exhaustiveness enforcement** in parser — `NonExhaustiveMatch` error if any of -1/0/+1 missing
- [x] **Codegen**: `ForIn`, `Loop`+`Break`, `WhileTernary`, `Use` (no-op), `Continue` (no-op), `BinOp` operators
- [x] **Semantic checker**: all new nodes handled
- [ ] **Standard Library** source files: `std::trit`, `std::tensor`, `std::math`, `std::io`
- [ ] **Real function call type resolution** in semantic checker (replace mock)
- [ ] `cast()` expression for bool→trit coercion
- [ ] `struct` definitions and field access

---

## 🔌 Phase 3.5: MCP Integration — COMPLETE ✅
**Any binary AI agent connected to this MCP server becomes a ternary decision engine.**

- [x] `ternlang-mcp` crate — JSON-RPC 2.0 over stdio, MCP protocol 2024-11-05
- [x] `trit_decide` — flagship tool: float evidence → ternary decision (+1/0/-1) with confidence, interpretation, sparsity
- [x] `trit_consensus` — consensus(a, b) with carry
- [x] `trit_eval` — evaluate ternlang expressions on live BET VM
- [x] `ternlang_run` — compile + run full .tern programs via MCP
- [x] `quantize_weights` — f32 → ternary with BitNet thresholding
- [x] `sparse_benchmark` — sparse vs dense matmul stats
- [x] `mcp-config.json` — drop-in config for Claude Desktop and any MCP client
- [x] Release binary: `target/release/ternlang-mcp`

**Next for MCP:** publish to MCP registry, write integration guide

---

## 🤖 Phase 5: Actor Model & Distributed Agents
**Keywords exist in spec but NOT in lexer/parser/AST yet.**
- [ ] **Lexer/Parser/AST**: `agent`, `spawn`, `send`, `await`, `remote`, `nodeid`, `agentref`
- [ ] **Local Actor Runtime**: actor registry, `TSPAWN`/`TSEND`/`TAWAIT` opcodes, green threads
- [ ] **Distributed Runtime**: serialize agentref as (nodeid, local_id), TCP transport, later libp2p

---

## 📡 Phase 6: Hardware & HDL Backends
- [ ] **Verilog/VHDL Codegen**: `ternlang-hdl` crate, map trit → 2-bit wire pairs
- [ ] **BET ISA Spec Document**: formal published standard (citable by academics)
- [ ] **FPGA Simulation**: Verilator/Icarus Verilog wrapper for BET bytecode

---

## 🛠 Developer Tooling (Parallel Track)
- [ ] **LSP**: `ternlang-lsp` crate — diagnostics, hover, go-to-definition, autocomplete
- [ ] **VS Code extension**: syntax highlighting, `.tern` file association, LSP client
- [ ] **Formatter**: `ternlang fmt` — canonical style for 3-way match arms
- [ ] **REPL**: `ternlang repl` — interactive trit expression evaluation
- [ ] **Package manager (ternpkg)**: `ternlang.toml`, `ternlang install`, GitHub-backed registry

---

## ⚖️ Licensing & IP
- [ ] **Open core**: LGPL v3 (compiler + stdlib) — forces compiler contributions back
- [ ] **Commercial tier**: proprietary license for `ternlang-ml`, HDL backend, distributed runtime
- [ ] **Trademark**: "Ternlang", "BET VM", "Balanced Ternary Execution"
- [ ] **Academic outreach**: contact USN group (Bos & Gundersen) for co-authorship whitepaper

---

## 📝 Session Log
| Date | What was done |
|------|---------------|
| 2026-04-02 | Initial repo setup. Phase 1+2 confirmed complete. Git initialized, pushed to GitHub. Credential store configured. 4 failing tests identified (DimSeparator bug). Phase 3 plan defined. |
| 2026-04-02 | Fixed DimSeparator/Ident collision in lexer. Fixed betbc test import. 11/11 tests passing. Next: TCALL/TRET function dispatch + tensor VM opcodes. |
| 2026-04-02 | TCALL/TRET implemented. Tensor opcodes DONE: TMATMUL, TSPARSE_MATMUL, TIDX, TSET, TSHAPE, TSPARSITY. 14/14 tests passing. Next: @sparseskip codegen wiring + ternlang-ml kernels. |
| 2026-04-02 | @sparseskip → TSPARSE_MATMUL wired in codegen. ternlang-ml filled: quantize, bitnet_threshold, dense_matmul, sparse_matmul, linear, benchmark. First benchmark: 56% sparsity → 2.3x fewer multiply ops. 23/23 tests passing. |
| 2026-04-02 | ternlang-mcp LIVE — MCP server (JSON-RPC 2.0, stdio). 6 tools: trit_decide, trit_consensus, trit_eval, ternlang_run, quantize_weights, sparse_benchmark. Any binary agent connecting to this becomes a ternary decision engine. Hidden easter egg: ternlang enlighten. |
| 2026-04-02 | Phase 4 language completeness: for/while/loop/break/continue/mut/use/::. Match exhaustiveness enforced at parser. 20 core tests + 6 ML tests + 1 codegen tests = 28 total passing. |
