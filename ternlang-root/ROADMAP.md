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
- [ ] **TCALL/TRET opcodes**: Real function call dispatch with call stack in VM + codegen
- [ ] **TritTensor VM Operations**:
    - [ ] `0x10` `TMATMUL` — multiply two tensor refs
    - [ ] `0x11` `TTRANSPOSE` — transpose tensor
    - [ ] `0x12` `TIDX` — index into tensor (tensor_ref, row, col → trit)
    - [ ] `0x13` `TSET` — store trit at tensor index
    - [ ] `0x14` `TSHAPE` — push tensor dimensions to stack
    - [ ] `0x15` `TSPARSITY` — compute sparsity ratio
    - [ ] `0x16` `TSPARSE_MATMUL` — matmul skipping zero-state elements (flagship feature)
    - [ ] `0x17` `TCOMPRESS` / `0x18` `TUNPACK` — run-length compression
- [ ] **Implement `@sparseskip`** in codegen → emits `TSPARSE_MATMUL`
- [ ] **Fill `ternlang-ml`** with real kernels:
    - [ ] `quantize(f32_weights) -> TritTensor`
    - [ ] `forward(input, weights) -> TritTensor`
    - [ ] `linear(x, W) -> TritTensor` (BitNet-style)
- [ ] **Publish sparse matmul benchmark** vs float32 (first public proof of concept)

---

## 🧩 Phase 4: Language Completeness
- [ ] **Add to Lexer/Parser/AST**: `for`, `while`, `loop`, `mut`, `struct`, `string`, `float`, `cast()`
- [ ] **Module system**: `use std::trit;`, `::` namespace access
- [ ] **Standard Library** source files: `std::trit`, `std::tensor`, `std::math`, `std::io`
- [ ] **Match exhaustiveness enforcement** in semantic checker
- [ ] **Real function call type resolution** in semantic checker (replace mock)

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
