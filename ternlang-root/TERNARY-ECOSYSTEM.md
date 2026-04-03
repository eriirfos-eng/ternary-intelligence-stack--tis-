# The Ternary Computing Ecosystem

**RFI-IRFOS Ternary Intelligence Stack — Compatibility & Convergence Map**

> Balanced ternary computing has been developed in fragments across universities, hobbyist communities, and quantum research labs. This document maps the field and shows how each effort relates to ternlang — not as competition, but as potential interoperability targets.

---

## Why Ternlang is the Convergence Point

Every existing ternary project solves one slice of the problem:
- Emulators with no compiler
- Compilers with no runtime
- Academic hardware with no software ecosystem
- ML experiments with no ISA

Ternlang provides the **full vertical stack**: language → compiler → VM → ML kernels → HDL → distributed runtime → tooling. It is the natural hub because it is the only project that can *talk to all the others*.

---

## Active Projects in the Field

### Brandon Smith's 9-Trit Simulator
**Architecture:** 9-trit words (range: −9841 to +9841 = 3⁹), RISC-like assembly, Python implementation  
**Components:** `trit.py`, `gates.py`, `alu.py` — clean balanced ternary carry propagation  
**File extension:** `.tern` (shared with ternlang)  
**Relation to ternlang:**
- The 9-trit word model is a specific ISA choice; BET uses 27 registers with 2-bit encoded trits
- Interop path: a `.tasm` → BET bytecode assembler shim (planned: `ternlang-compat` crate)
- His carry propagation logic is mathematically equivalent to our `TADD` opcode — just differently encoded

### University of South-Eastern Norway — Bos & Gundersen
**Work:** C-to-ternary assembly compilers, Electronic Design Automation (EDA) tools for ternary circuits, memristor-backed ternary state storage  
**Paper focus:** Logic synthesis for non-binary states  
**Tools:** `uMemristorToolbox` (Unity-integrated, controls physical memristors for ternary storage)  
**Relation to ternlang:**
- Their C-to-ternary approach leaks binary assumptions; ternlang's native syntax eliminates the abstraction gap
- Their EDA work and ternlang-hdl solve the same problem from different angles — collaboration target
- Academic outreach planned: co-authorship on a whitepaper comparing BET ISA to their EDA output
- `uMemristorToolbox` is a potential Phase 7 hardware target for ternlang programs

### Owlet
**Architecture:** S-expression (Lisp-like) syntax, Node.js runtime, `.owlet` extension  
**Paradigm:** Every number is signed by default; recursive expression evaluation in balanced ternary  
**Relation to ternlang:**
- S-expressions are a mathematically clean IR — ideal as an alternative syntax front-end
- Planned: `ternlang-owlet` front-end crate that parses S-expressions into ternlang AST nodes
- This would let Owlet programs compile and run on the BET VM

### Trit-Rust
**Architecture:** Rust crate, `i8`-backed trit values, ternary logic simulation  
**Relation to ternlang:**
- ternlang-core already supersedes this with a full VM + codegen
- Planned: publish `ternlang-trit` to crates.io as the definitive Rust ternary primitives crate

### T-CPU Assembler (ternary-computing.com)
**Architecture:** Assembler for the 5500FP hardware; balanced ternary literal syntax (e.g. `10T` = 8 decimal)  
**Relation to ternlang:**
- Syntax reference for ternlang's trit literal notation
- Planned: ternlang can emit `.tasm`-compatible assembly as an alternative target backend

### Q-Ternary / Qutrits
**Architecture:** DSL for simulating qutrits (3-state quantum bits), targeting hardware like Google's Willow chip  
**Relation to ternlang:**
- `trittensor<N×M>` naturally maps to qutrit state spaces — ternlang is already qutrit-adjacent
- The BET encoding (`0b01`=−1, `0b10`=+1, `0b11`=0) can represent qutrit basis states `|−⟩`, `|+⟩`, `|0⟩`
- Documentation planned: "ternlang as a qutrit programming model" section in BET-ISA-SPEC.md

---

## Compatibility Roadmap

### Phase 7A — Interop Bridges
- [ ] `ternlang-compat` crate: `.tasm` (9-trit assembly) → BET bytecode assembler
- [ ] `ternlang-owlet` crate: S-expression front-end → ternlang AST → BET VM
- [ ] `ternlang-trit` published to crates.io (from ternlang-core primitives)

### Phase 7B — Academic & Hardware
- [ ] Whitepaper: "BET: A Complete Balanced Ternary Execution Architecture" — submit to ArXiv
- [ ] Contact USN group for co-authorship on logic synthesis comparison
- [ ] `uMemristorToolbox` driver interface for physical ternary state storage

### Phase 7C — Quantum Bridge
- [ ] Document `trittensor` as qutrit state model in BET-ISA-SPEC.md
- [ ] Explore Q-Ternary → ternlang compilation path

---

## The Bigger Picture

```
                    ┌─────────────────────────┐
                    │   ternlang / BET VM      │  ← THE HUB
                    │   RFI-IRFOS TIS          │
                    └───────────┬─────────────┘
                                │
          ┌─────────────────────┼─────────────────────┐
          │                     │                     │
   ┌──────▼──────┐    ┌─────────▼────────┐   ┌───────▼──────┐
   │  9-trit sim  │    │  Owlet (S-expr)  │   │  T-CPU .tasm  │
   │  .tasm shim  │    │  front-end crate │   │  assembly     │
   └─────────────┘    └──────────────────┘   └──────────────┘
          │                     │                     │
   ┌──────▼──────┐    ┌─────────▼────────┐   ┌───────▼──────┐
   │  USN / EDA   │    │  Trit-Rust crate │   │  Q-Ternary   │
   │  whitepaper  │    │  → crates.io     │   │  qutrit docs │
   └─────────────┘    └──────────────────┘   └──────────────┘
```

Every arrow is a real compatibility bridge we will build. The BET ISA is the common ground that makes them all interoperable.

---

*RFI-IRFOS — Ternary Intelligence Stack*  
*"The place where the fractured ternary field compiles into something whole."*
