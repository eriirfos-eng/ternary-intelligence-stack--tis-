//! ternlang-hdl — Phase 6: Hardware Description Language backend
//!
//! Maps ternlang/BET bytecode to synthesisable Verilog/VHDL.
//!
//! ## Trit → 2-bit wire pair encoding
//!
//! BET uses 2-bit balanced ternary encoding:
//!
//!   0b01 (-1) → wire pair: t1=0, t0=1   (conflict)
//!   0b10 (+1) → wire pair: t1=1, t0=0   (truth)
//!   0b11 ( 0) → wire pair: t1=1, t0=1   (hold)
//!   0b00       FAULT — invalid state
//!
//! Each ternary variable becomes a `[1:0]` bus in Verilog.
//!
//! ## Modules generated
//! - `trit_neg`    — inversion
//! - `trit_cons`   — consensus (ternary OR)
//! - `trit_mul`    — ternary multiply
//! - `trit_add`    — balanced ternary adder with carry
//! - `trit_reg`    — ternary D-register (synchronous, reset to hold)
//! - `bet_alu`     — full BET ALU
//! - Sparse matmul array (parameterised N×N)

pub mod verilog;
pub mod isa;
pub mod sim;
pub mod rtl_sim;

pub use verilog::{VerilogEmitter, VerilogModule};
pub use isa::BetIsaEmitter;
pub use sim::BetSimEmitter;
pub use rtl_sim::{BetRtlProcessor, RtlTrace, TritWire};
