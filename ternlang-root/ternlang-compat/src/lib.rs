//! ternlang-compat — Compatibility bridges for the ternary computing ecosystem
//!
//! This crate is the convergence point for existing ternary computing projects,
//! making ternlang the common runtime they all target.
//!
//! ## Bridges implemented
//!
//! - **`tasm`** — 9-trit RISC assembly → BET bytecode assembler
//!   Translates the Brandon Smith / ternary-computing.com `.tasm` assembly
//!   dialect (9-trit words, RISC-like mnemonics) into BET VM bytecode.
//!
//! - **`owlet`** — S-expression ternary front-end
//!   Parses Owlet-style S-expressions into ternlang AST nodes for evaluation
//!   on the BET VM.
//!
//! ## 9-trit word model
//!
//! The `.tasm` ecosystem uses 9-trit words (range −9841 to +9841 = 3⁹).
//! Each trit is one of {−1, 0, +1}. Literals use 'T' for −1 (e.g. `10T` = 8).
//!
//! BET mapping:
//! - Registers r0–r8 → BET registers 0–8
//! - LOAD/STORE → TLOAD (0x07) / TSTORE (0x08)
//! - ADD → TADD (0x02)
//! - NEG → TNEG (0x04)
//! - MUL → TMUL (0x03)
//! - JMP → TJMP (0x0b)
//! - BEQ (branch if zero) → TJMP_ZERO (0x06)
//! - BLT (branch if neg)  → TJMP_NEG  (0x07)
//! - HALT → THALT (0x00)
//! - NOP  → no-op (emit nothing)

pub mod tasm;
pub mod owlet;

pub use tasm::{TasmAssembler, TasmError};
pub use owlet::OwletParser;
