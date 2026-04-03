# Ternlang Roadmap: Bridging the Ternary Software Deficit
### Project: Ternary Intelligence Stack (TIS) | RFI-IRFOS
**Current Version:** v0.1 (Foundational)
**Last Updated:** 2026-04-03
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

### 📄 Whitepaper Update Protocol
The academic whitepaper (`whitepaper/ternlang-whitepaper.tex` + `whitepaper/ternlang-whitepaper.docx`) is a **living document**. Update it whenever a phase produces measurable results:
- New opcodes or VM features → update Section 4 (ISA) and Section 10 (implementation status)
- New benchmark numbers → update Section 5 (sparse inference) tables
- New crates or test counts → update Table 8 (implementation status)
- Rebuild DOCX: `python3 whitepaper/build_docx.py`
- Rebuild PDF: `cd whitepaper && pdflatex ternlang-whitepaper.tex` (requires texlive)

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
- [x] **`TCOMPRESS` (0x26) / `TUNPACK` (0x27)** — base-3 RLE codec for sparse trit tensors; max-chunk=8 (2 base-3 digits), NegOne header; 5 VM tests
- [x] **Fill `ternlang-ml`** with real kernels — DONE:
    - [x] `quantize(f32_weights, threshold) -> Vec<Trit>` — BitNet-style ternary quantization
    - [x] `bitnet_threshold(weights)` — auto-compute τ = 0.5 × mean(|w|)
    - [x] `dense_matmul(a, b) -> TritMatrix` — baseline
    - [x] `sparse_matmul(a, b) -> (TritMatrix, skipped_count)` — flagship kernel
    - [x] `linear(input, W) -> (TritMatrix, skipped)` — BitNet-style ternary linear layer
    - [x] `benchmark(a, b) -> BenchmarkResult` — prints summary with skip rate
- [x] **First benchmark result**: 56% weight sparsity → **2.3x fewer multiply ops** vs dense
- [x] **Wall-clock timing benchmark**: 5 sizes (32²–512²), 5-rep median
- [x] **CSC sparse matmul**: Compressed Sparse Column precompute — branch-free inner loop; at 25% sparsity: **4.8–6.9× faster**; at 60% sparsity (BitNet-realistic): **8–14× faster** than dense in release mode
- [x] **BitNet b1.58 benchmark**: explicit 60% sparsity, release mode — **86×** at 512² (3-layer CSC kernel)
- [x] **Goldilocks sparsity sweep**: 9 sparsity levels × 5 sizes — peak **122×** at 99% sparsity 512²; goldilocks zone confirmed at 40–60% sparsity (20–57× on medium matrices)
- [x] **TernaryMLP**: 2-layer MLP (from_f32, forward, predict, XOR/parity datasets, accuracy eval) — full inference path tested end-to-end
- [ ] **Publish sparse matmul benchmark** — write blog post / README section comparing vs float32

---

## 🧩 Phase 4: Language Completeness — IN PROGRESS
- [x] **Lexer**: `for`, `in`, `while`, `loop`, `break`, `continue`, `mut`, `use`, `module`, `pub`, `struct`, `::`, `!=`, `&&`, `||`
- [x] **AST**: `ForIn`, `WhileTernary`, `Loop`, `Break`, `Continue`, `Use` nodes; `BinOp::NotEqual/And/Or`; `Type::Bool/Float/String`
- [x] **Parser**: `for x in expr { }`, `while cond ? { } else { } else { }`, `loop { }`, `break`, `continue`, `use std::trit;`, `let mut`
- [x] **Match exhaustiveness enforcement** in parser — `NonExhaustiveMatch` error if any of -1/0/+1 missing
- [x] **Codegen**: `ForIn`, `Loop`+`Break`, `WhileTernary`, `Use` (no-op), `Continue` (no-op), `BinOp` operators
- [x] **Semantic checker**: all new nodes handled
- [x] **Standard Library** source files: `std::trit`, `std::tensor`, `std::math`, `std::io`, `ml::quantize`, `ml::inference`
- [x] **StdlibLoader**: `use std::trit;` inside function bodies actually injects parsed stdlib functions — `include_str!` at compile time, zero runtime filesystem I/O
- [x] **Comment support in lexer**: `//` line comments now skipped — user programs and stdlib files can use comments freely
- [x] **Real function call type resolution** in semantic checker (FunctionSig exact/variadic, ArgCountMismatch, ArgTypeMismatch, ReturnTypeMismatch)
- [x] `cast()` expression for bool→trit coercion — transparent BET pass-through, type-system level only
- [x] `struct` definitions and field access — `struct Name {}`, `s.field`, `s.field = v;`, `Type::Named`

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

## 🤖 Phase 5: Actor Model & Distributed Agents — PHASE 5.1 COMPLETE ✅
- [x] **Lexer/Parser/AST**: `agent`, `spawn`, `send`, `await`, `agentref` — all done
- [x] **Local Actor Runtime**: AgentInstance + mailbox, `TSPAWN`/`TSEND`/`TAWAIT` opcodes, synchronous dispatch
- [x] **Integration test**: spawn identity-agent, send +1, await → +1 ✓
- [x] **Distributed Runtime** (Phase 5.1): `RemoteTransport` trait in core (no circular dep), `TernNode` impl in runtime; TSEND/TAWAIT route over TCP for remote AgentRefs; auto-connect on first use; 4 runtime tests passing
- [x] **`remote`/`nodeid`** keywords: `--node-addr` + `--peer` CLI flags; `TernNode` injected into VM via `set_remote(Arc<dyn RemoteTransport>)`

---

## 📡 Phase 6: Hardware & HDL Backends — PHASE 6.1 COMPLETE ✅
- [x] **Verilog/VHDL Codegen**: `ternlang-hdl` crate, map trit → 2-bit wire pairs
  - Primitives: trit_neg, trit_cons, trit_mul, trit_add, trit_reg, bet_alu
  - Sparse matmul array: parameterised N×N with per-cell zero-skip enable
  - ISA control: bet_regfile (27 reg), bet_pc (16-bit), bet_control (all opcodes), bet_processor (top-level)
  - 11 HDL tests passing
- [x] **BET ISA Spec Document**: `BET-ISA-SPEC.md` — formal ISA spec with encoding tables, stack-effect notation, hardware mapping
- [x] **FPGA Simulation** (Phase 6.1): Cycle-accurate RTL simulator in pure Rust (`BetRtlProcessor`) — mirrors `bet_processor.v` exactly; same 2-bit encoding, clocked regfile, PC, ALU; 12 RTL unit tests; `ternlang sim --rtl` CLI flag (no external tools needed); iverilog path still supported via `ternlang sim --run`

---

## 🛠 Developer Tooling — COMPLETE ✅
- [x] **LSP**: `ternlang-lsp` crate — JSON-RPC 2.0 over stdio, diagnostics, hover, completion (19 snippets)
- [x] **VS Code extension**: `ternlang-vscode/` — TextMate grammar, .tern file association, LSP client
- [x] **Formatter**: `ternlang fmt [--write]` — canonical style for 3-way match arms
- [x] **REPL**: `ternlang repl` — interactive trit expression evaluation via BET VM
- [x] **Package manager (ternpkg)**: `ternlang.toml`, `ternpkg install [PKG]`, GitHub-backed registry

---

## 🌐 Phase 7: Ecosystem Bridges — PHASE 7A COMPLETE ✅
**Goal: make ternlang the convergence point for all existing ternary computing projects.**

- [x] **Hub positioning**: README + TERNARY-ECOSYSTEM.md — maps every active ternary project to a ternlang interop bridge
- [x] **TasmAssembler** (ternlang-compat): two-pass 9-trit RISC assembler → BET bytecode; parses balanced ternary literals (T=-1); 15 tests
- [x] **OwletParser** (ternlang-compat): S-expression ternary front-end → ternlang AST → BET VM; full S-expr grammar; 14 tests
- [x] **VS Code VSIX packaging**: `ternlang-0.1.0.vsix` built, publisher metadata set (rfi-irfos)
- [x] **Cargo workspace metadata**: `[workspace.package]` with keywords, categories, license, repository for crates.io
- [x] **Academic whitepaper**: `whitepaper/ternlang-whitepaper.tex` (IEEE two-column LaTeX) + `ternlang-whitepaper.docx`
- [x] **Spec consolidation**: `spec/grammar.ebnf`, `spec/ternlang-language-reference-v0.1.md`, `spec/ternlang-dictionary-v0.1.json` versioned in main repo
- [ ] **Phase 7B**: VS Code Marketplace publication (needs user publisher PAT token → `vsce publish`)
- [ ] **Phase 7B**: crates.io publication (`cargo login` + `cargo publish -p ternlang-core` etc.)
- [ ] **Phase 7B**: MCP registry publication, integration guide
- [ ] **Phase 7C**: USN / Bos+Gundersen academic outreach, joint whitepaper draft

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
| 2026-04-02 | stdlib source files: std::trit, std::math, std::tensor, std::io, ml::quantize, ml::inference. Struct defs + field access (s.field) + field assignment (s.field=v) + cast() + Type::Named. Dot token in lexer. 25/25 tests passing. |
| 2026-04-02 | Phase 5.0 actor model: agent/spawn/send/await/agentref in lexer+AST+parser+semantic+codegen+VM. TSPAWN/TSEND/TAWAIT opcodes. AgentInstance with mailbox. Integration test: spawn echo agent, send +1, await +1. 30/30 tests passing. |
| 2026-04-02 | Phase 5.1: ternlang-runtime crate (TCP distributed actors). TernNode with listen/connect/remote_send/remote_await. Wire protocol: newline JSON over TCP. remote/nodeid keywords. spawn remote "addr" syntax. StringLit token. Real function call type resolution in semantic checker. 31 core + 2 runtime tests. |
| 2026-04-02 | Phase 6.0: ternlang-hdl crate. Verilog primitives: trit_neg/cons/mul/add/reg, bet_alu, sparse_matmul(N). ISA control: bet_regfile/pc/control/processor. All BET opcodes mapped. 52 total tests passing. |
| 2026-04-03 | BET-ISA-SPEC.md formal spec published. ternlang-lsp: full LSP 3.17 JSON-RPC (hover, completion, diagnostics). ternlang-vscode: TextMate grammar, LSP client extension. ternlang fmt + repl in CLI. ternpkg v0.1: init/install/list/info, GitHub-backed registry. 58 total tests passing. |
| 2026-04-03 | Phase 7A: TasmAssembler + OwletParser (ternlang-compat, 29 tests). TCOMPRESS/TUNPACK RLE codec (0x26/0x27). TernaryMLP 2-layer with from_f32/forward/predict, XOR+parity datasets. timed_benchmark: 32²–512², 5-rep median wall-clock. BET sim emitter (Icarus Verilog testbench). Hub README + TERNARY-ECOSYSTEM.md. VSIX packaging. Whitepaper TEX+DOCX published (10 sections, IEEE two-column). Spec files consolidated into main repo. 116 total tests passing. |
| 2026-04-03 | StdlibLoader: `use std::trit;` works end-to-end. Comment skip in lexer. 3-layer CSC sparse matmul (flat i8 + offset table + Rayon): 86× at 60% sparsity, 122× at 99% sparsity (512² release). Goldilocks sweep confirms 40–60% as optimal zone for medium matrices. Whitepaper updated with full sweep table. 120+ tests passing. |
| 2026-04-03 | Multi-tenant API key management in ternlang-api: KeyStore (JSON-backed, async RwLock), key generation (tern_<tier>_<uuid24>), revocation, usage counters, admin routes POST/GET/DELETE /admin/keys. `TERNLANG_ADMIN_KEY` + `KEYS_FILE` env vars. Albert-agent integrated as primary TIS agent. 5 VM compile errors fixed (Value::Clone, AgentRef 2-tuple). Build clean across full workspace. |
| 2026-04-03 | Phase 5.1 COMPLETE: RemoteTransport trait in ternlang-core (no circular dep), TernNode impl in ternlang-runtime; TSEND/TAWAIT route over TCP for remote AgentRefs with auto-connect; `ternlang run --node-addr --peer` CLI flags wire TernNode into VM at startup; 4 runtime tests passing. |
| 2026-04-03 | Phase 6.1 COMPLETE: BetRtlProcessor — cycle-accurate RTL simulator in pure Rust. Mirrors bet_processor.v exactly: TritWire 2-bit encoding, trit_neg/cons/mul/add combinational primitives, BetRegfile (27 regs), BetPc (16-bit), BetAlu, bet_decode control unit. `ternlang sim --rtl [--max-cycles N]` CLI. 12 RTL unit tests + 2 doctests. 93 tests total across core/hdl/runtime. |
