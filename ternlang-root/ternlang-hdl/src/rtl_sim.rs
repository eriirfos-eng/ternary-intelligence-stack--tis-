//! BET Processor RTL Simulator — Phase 6.1
//!
//! A cycle-accurate Rust model of the BET processor Verilog.
//! Mirrors every module in `ternlang-hdl/src/isa.rs` / `verilog.rs` with the
//! same 2-bit trit encoding and clocked-register semantics.
//!
//! Encoding (matches Verilog):
//!   0b01 (1) → −1  (conflict / NegOne)
//!   0b10 (2) → +1  (truth   / PosOne)
//!   0b11 (3) →  0  (hold    / Zero)
//!   0b00 (0) →  FAULT (invalid)
//!
//! # Usage
//! ```no_run
//! use ternlang_hdl::BetRtlProcessor;
//! let bytecode = vec![0x01, 0b10, 0x01, 0b01, 0x02, 0x08, 0x00, 0x00];
//! let mut proc = BetRtlProcessor::new(bytecode);
//! let trace = proc.run(1000); // up to 1000 clock cycles
//! println!("cycles: {}, halted: {}", trace.cycles, trace.halted);
//! ```

// ─── 2-bit trit wire type ─────────────────────────────────────────────────────

/// A 2-bit balanced ternary wire value, matching the Verilog encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TritWire(pub u8);

impl TritWire {
    pub const NEG:  TritWire = TritWire(0b01); // -1, conflict
    pub const POS:  TritWire = TritWire(0b10); // +1, truth
    pub const HOLD: TritWire = TritWire(0b11); //  0, hold
    pub const FAULT:TritWire = TritWire(0b00); // invalid

    /// Convert from signed trit (-1, 0, +1).
    pub fn from_i8(v: i8) -> Self {
        match v {
            -1 => Self::NEG,
             1 => Self::POS,
             0 => Self::HOLD,
             _ => Self::FAULT,
        }
    }

    /// Convert to signed trit.
    pub fn to_i8(self) -> i8 {
        match self.0 {
            0b01 => -1,
            0b10 =>  1,
            0b11 =>  0,
            _    =>  0, // FAULT → hold (safe)
        }
    }

    pub fn is_hold(self) -> bool { self == Self::HOLD }
    pub fn is_pos(self)  -> bool { self == Self::POS  }
    pub fn is_neg(self)  -> bool { self == Self::NEG  }
}

impl std::fmt::Display for TritWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_i8())
    }
}

// ─── Combinational primitives (mirror trit_neg / trit_cons / trit_mul / trit_add) ──

/// `trit_neg`: invert by swapping bit pair.
/// Verilog: `assign y = {a[0], a[1]};`
pub fn trit_neg(a: TritWire) -> TritWire {
    TritWire(((a.0 & 1) << 1) | ((a.0 >> 1) & 1))
}

/// `trit_cons`: consensus (ternary OR).
/// Verilog: `assign y = (a == b) ? a : 2'b11;`
pub fn trit_cons(a: TritWire, b: TritWire) -> TritWire {
    if a == b { a } else { TritWire::HOLD }
}

/// `trit_mul`: balanced ternary multiply.
/// Verilog: same-encoding signs → POS, different → NEG, either HOLD → HOLD.
pub fn trit_mul(a: TritWire, b: TritWire) -> TritWire {
    if a.is_hold() || b.is_hold() { return TritWire::HOLD; }
    if a == b { TritWire::POS } else { TritWire::NEG }
}

/// `trit_add`: balanced ternary adder with carry.
/// Returns (sum, carry).
/// Truth table matches the `trit_add.v` Verilog module.
pub fn trit_add(a: TritWire, b: TritWire) -> (TritWire, TritWire) {
    let sum = a.to_i8() + b.to_i8();
    match sum {
        -2 => (TritWire::POS,  TritWire::NEG),  // -2 = +1 + carry(-1)
        -1 => (TritWire::NEG,  TritWire::HOLD),
         0 => (TritWire::HOLD, TritWire::HOLD),
         1 => (TritWire::POS,  TritWire::HOLD),
         2 => (TritWire::NEG,  TritWire::POS),  //  2 = -1 + carry(+1)
         _ => (TritWire::HOLD, TritWire::HOLD),
    }
}

// ─── BET Register File (bet_regfile.v) ───────────────────────────────────────

/// 27-register × 2-bit ternary register file.
/// Synchronous write, asynchronous read — matches the Verilog.
pub struct BetRegfile {
    regs: [TritWire; 27],
}

impl BetRegfile {
    pub fn new() -> Self {
        Self { regs: [TritWire::HOLD; 27] } // reset → hold (0)
    }

    pub fn read(&self, addr: u8) -> TritWire {
        self.regs.get(addr as usize).copied().unwrap_or(TritWire::HOLD)
    }

    /// Clocked write (positive edge).
    pub fn write(&mut self, addr: u8, data: TritWire) {
        if (addr as usize) < 27 {
            self.regs[addr as usize] = data;
        }
    }

    pub fn dump(&self) -> Vec<i8> {
        self.regs.iter().map(|t| t.to_i8()).collect()
    }
}

// ─── BET Program Counter (bet_pc.v) ──────────────────────────────────────────

/// 16-bit program counter. Clocked: load → PC ← next_pc, else PC ← PC+1.
pub struct BetPc {
    pub pc: u16,
}

impl BetPc {
    pub fn new() -> Self { Self { pc: 0 } }

    /// Tick: if `load` then jump to `next_pc`, else increment.
    pub fn tick(&mut self, load: bool, next_pc: u16) {
        if load { self.pc = next_pc; } else { self.pc = self.pc.wrapping_add(1); }
    }
}

// ─── BET ALU (bet_alu.v) ─────────────────────────────────────────────────────

/// ALU op codes — match `alu_op[1:0]` in Verilog control unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp { Add = 0, Mul = 1, Neg = 2, Cons = 3 }

/// Execute one ALU operation.
pub fn bet_alu(op: AluOp, a: TritWire, b: TritWire) -> (TritWire, TritWire) {
    match op {
        AluOp::Add  => trit_add(a, b),
        AluOp::Mul  => (trit_mul(a, b), TritWire::HOLD),
        AluOp::Neg  => (trit_neg(a),    TritWire::HOLD),
        AluOp::Cons => (trit_cons(a, b), TritWire::HOLD),
    }
}

// ─── Control signals (bet_control.v) ─────────────────────────────────────────

/// Decoded control signals for one opcode — matches `bet_control.v` outputs.
#[derive(Debug, Clone, Copy)]
pub struct ControlSignals {
    pub alu_op:   AluOp,
    pub reg_we:   bool,  // register file write enable
    pub pc_load:  bool,  // unconditional jump
    pub is_push:  bool,  // TPUSH: push literal to stack
    pub is_halt:  bool,  // THALT
    pub is_store: bool,  // TSTORE
    pub is_load:  bool,  // TLOAD
    pub is_jmp:   bool,  // conditional jump (TJMPPOS/ZERO/NEG)
    pub jmp_cond: JmpCond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JmpCond { Pos, Zero, Neg, Always }

impl ControlSignals {
    fn noop() -> Self {
        Self {
            alu_op: AluOp::Add, reg_we: false, pc_load: false,
            is_push: false, is_halt: false, is_store: false, is_load: false,
            is_jmp: false, jmp_cond: JmpCond::Always,
        }
    }
}

/// Decode one opcode byte → control signals.
/// Mirrors the `always @(*)` block in `bet_control.v`.
pub fn bet_decode(opcode: u8) -> ControlSignals {
    let mut c = ControlSignals::noop();
    match opcode {
        0x00 => { c.is_halt = true; }
        0x01 => { c.is_push = true; }                                      // TPUSH
        0x02 => { c.alu_op = AluOp::Add;  c.reg_we = false; }             // TADD
        0x03 => { c.alu_op = AluOp::Mul;  c.reg_we = false; }             // TMUL
        0x04 => { c.alu_op = AluOp::Neg;  c.reg_we = false; }             // TNEG
        0x05 => { c.is_jmp = true; c.jmp_cond = JmpCond::Pos;    }        // TJMPPOS
        0x06 => { c.is_jmp = true; c.jmp_cond = JmpCond::Zero;   }        // TJMPZERO
        0x07 => { c.is_jmp = true; c.jmp_cond = JmpCond::Neg;    }        // TJMPNEG
        0x08 => { c.is_store = true; }                                     // TSTORE
        0x09 => { c.is_load  = true; }                                     // TLOAD
        0x0b => { c.is_jmp = true; c.jmp_cond = JmpCond::Always; }        // TJMP
        0x0e => { c.alu_op = AluOp::Cons; c.reg_we = false; }             // TCONS
        _    => {} // unknown → no-op (safe)
    }
    c
}

// ─── Cycle trace ──────────────────────────────────────────────────────────────

/// Snapshot of processor state at the end of one clock cycle.
#[derive(Debug, Clone)]
pub struct CycleState {
    pub cycle:   u64,
    pub pc:      u16,
    pub opcode:  u8,
    pub stack:   Vec<i8>,        // stack contents as signed trits
    pub carry:   i8,
    pub regs:    Vec<i8>,        // first 10 registers
}

/// Full execution trace returned by `BetRtlProcessor::run()`.
#[derive(Debug)]
pub struct RtlTrace {
    pub cycles:  u64,
    pub halted:  bool,
    pub cycles_state: Vec<CycleState>,
    pub final_regs:   Vec<i8>,   // all 27 registers
    pub final_stack:  Vec<i8>,
}

// ─── Top-level processor (bet_processor.v) ───────────────────────────────────

/// Cycle-accurate BET processor simulation.
///
/// Implements the same single-cycle fetch-decode-execute pipeline as
/// `bet_processor.v`, using the combinational primitives above.
pub struct BetRtlProcessor {
    pub regfile:  BetRegfile,
    pub pc:       BetPc,
    pub stack:    Vec<TritWire>,
    pub carry:    TritWire,
    pub code:     Vec<u8>,
    pub halted:   bool,
}

impl BetRtlProcessor {
    pub fn new(code: Vec<u8>) -> Self {
        Self {
            regfile: BetRegfile::new(),
            pc:      BetPc::new(),
            stack:   Vec::new(),
            carry:   TritWire::HOLD,
            code,
            halted:  false,
        }
    }

    fn fetch(&self) -> Option<u8> {
        self.code.get(self.pc.pc as usize).copied()
    }

    fn read_u16_at(&self, offset: usize) -> u16 {
        let lo = self.code.get(offset).copied().unwrap_or(0) as u16;
        let hi = self.code.get(offset + 1).copied().unwrap_or(0) as u16;
        lo | (hi << 8)
    }

    /// Execute one clock cycle.
    /// Returns false when THALT encountered or PC out of range.
    pub fn tick(&mut self) -> bool {
        let opcode = match self.fetch() {
            Some(op) => op,
            None => { self.halted = true; return false; }
        };
        let ctrl = bet_decode(opcode);
        let pc_now = self.pc.pc;

        if ctrl.is_halt {
            self.halted = true;
            return false;
        }

        let mut pc_load  = false;
        let mut next_pc  = pc_now.wrapping_add(1);

        if ctrl.is_push {
            // TPUSH: next byte is packed trit
            let byte = self.code.get(pc_now as usize + 1).copied().unwrap_or(0b11);
            // packed trit matches VM: 0b01=-1, 0b10=+1, 0b11=0
            let tw = TritWire(byte & 0b11);
            let tw = if tw == TritWire::FAULT { TritWire::HOLD } else { tw };
            self.stack.push(tw);
            self.pc.tick(false, 0);
            self.pc.tick(false, 0); // advance past the data byte
            return true;
        }

        if ctrl.is_store {
            let reg = self.code.get(pc_now as usize + 1).copied().unwrap_or(0);
            let val = self.stack.pop().unwrap_or(TritWire::HOLD);
            self.regfile.write(reg, val);
            self.pc.tick(false, 0);
            self.pc.tick(false, 0);
            return true;
        }

        if ctrl.is_load {
            let reg = self.code.get(pc_now as usize + 1).copied().unwrap_or(0);
            let val = self.regfile.read(reg);
            self.stack.push(val);
            self.pc.tick(false, 0);
            self.pc.tick(false, 0);
            return true;
        }

        if ctrl.is_jmp {
            let addr = self.read_u16_at(pc_now as usize + 1);
            let top  = self.stack.pop().unwrap_or(TritWire::HOLD);
            let take = match ctrl.jmp_cond {
                JmpCond::Always => true,
                JmpCond::Pos    => top.is_pos(),
                JmpCond::Zero   => top.is_hold(),
                JmpCond::Neg    => top.is_neg(),
            };
            if take { pc_load = true; next_pc = addr; }
            // If TJMP (unconditional), we still consumed the top — but unconditional
            // doesn't pop in the VM. Restore if Always and not consumed.
            if ctrl.jmp_cond == JmpCond::Always {
                // TJMP doesn't consume stack — put it back
                self.stack.push(top);
            }
            // Advance past the 2-byte address operand
            if !pc_load { next_pc = pc_now.wrapping_add(3); }
            self.pc.tick(pc_load, next_pc);
            return true;
        }

        // ALU operations: pop operands, push result
        match opcode {
            0x04 => { // TNEG: unary
                let a = self.stack.pop().unwrap_or(TritWire::HOLD);
                let (y, _) = bet_alu(AluOp::Neg, a, TritWire::HOLD);
                self.stack.push(y);
            }
            0x02 | 0x03 | 0x0e => { // TADD / TMUL / TCONS: binary
                let b = self.stack.pop().unwrap_or(TritWire::HOLD);
                let a = self.stack.pop().unwrap_or(TritWire::HOLD);
                let op = match opcode {
                    0x02 => AluOp::Add,
                    0x03 => AluOp::Mul,
                    _    => AluOp::Cons,
                };
                let (y, c) = bet_alu(op, a, b);
                self.stack.push(y);
                self.carry = c;
            }
            _ => {} // no-op for unknown
        }

        self.pc.tick(pc_load, next_pc);
        true
    }

    /// Run up to `max_cycles` clock ticks, recording a trace.
    pub fn run(&mut self, max_cycles: u64) -> RtlTrace {
        let mut states = Vec::new();

        for cycle in 0..max_cycles {
            // Snapshot before this tick
            let snap = CycleState {
                cycle,
                pc:     self.pc.pc,
                opcode: self.fetch().unwrap_or(0),
                stack:  self.stack.iter().map(|t| t.to_i8()).collect(),
                carry:  self.carry.to_i8(),
                regs:   self.regfile.regs[..10].iter().map(|t| t.to_i8()).collect(),
            };
            states.push(snap);

            if !self.tick() { break; }
        }

        RtlTrace {
            cycles:       states.len() as u64,
            halted:       self.halted,
            cycles_state: states,
            final_regs:   self.regfile.dump(),
            final_stack:  self.stack.iter().map(|t| t.to_i8()).collect(),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the 2-bit encoding constants match the Verilog spec.
    #[test]
    fn test_trit_wire_encoding() {
        assert_eq!(TritWire::NEG.to_i8(),  -1);
        assert_eq!(TritWire::POS.to_i8(),   1);
        assert_eq!(TritWire::HOLD.to_i8(),  0);
        assert_eq!(TritWire::from_i8(-1), TritWire::NEG);
        assert_eq!(TritWire::from_i8(1),  TritWire::POS);
        assert_eq!(TritWire::from_i8(0),  TritWire::HOLD);
    }

    /// trit_neg swaps bit pair: +1 → -1, -1 → +1, 0 → 0.
    #[test]
    fn test_trit_neg() {
        assert_eq!(trit_neg(TritWire::POS),  TritWire::NEG);
        assert_eq!(trit_neg(TritWire::NEG),  TritWire::POS);
        assert_eq!(trit_neg(TritWire::HOLD), TritWire::HOLD);
    }

    /// trit_cons: agree → output, disagree → hold.
    #[test]
    fn test_trit_cons() {
        assert_eq!(trit_cons(TritWire::POS,  TritWire::POS),  TritWire::POS);
        assert_eq!(trit_cons(TritWire::NEG,  TritWire::NEG),  TritWire::NEG);
        assert_eq!(trit_cons(TritWire::POS,  TritWire::NEG),  TritWire::HOLD);
        assert_eq!(trit_cons(TritWire::POS,  TritWire::HOLD), TritWire::HOLD);
        assert_eq!(trit_cons(TritWire::HOLD, TritWire::HOLD), TritWire::HOLD);
    }

    /// trit_mul: same sign → +1, different → -1, any 0 → 0.
    #[test]
    fn test_trit_mul() {
        assert_eq!(trit_mul(TritWire::POS,  TritWire::POS),  TritWire::POS);
        assert_eq!(trit_mul(TritWire::NEG,  TritWire::NEG),  TritWire::POS);
        assert_eq!(trit_mul(TritWire::POS,  TritWire::NEG),  TritWire::NEG);
        assert_eq!(trit_mul(TritWire::POS,  TritWire::HOLD), TritWire::HOLD);
        assert_eq!(trit_mul(TritWire::HOLD, TritWire::HOLD), TritWire::HOLD);
    }

    /// trit_add: balanced ternary adder with carry.
    #[test]
    fn test_trit_add() {
        let (s, c) = trit_add(TritWire::POS, TritWire::POS);
        assert_eq!(s.to_i8(), -1); // +1+1 = -1 carry+1
        assert_eq!(c.to_i8(),  1);

        let (s, c) = trit_add(TritWire::NEG, TritWire::NEG);
        assert_eq!(s.to_i8(),  1); // -1-1 = +1 carry-1
        assert_eq!(c.to_i8(), -1);

        let (s, c) = trit_add(TritWire::POS, TritWire::NEG);
        assert_eq!(s.to_i8(),  0);
        assert_eq!(c.to_i8(),  0);

        let (s, c) = trit_add(TritWire::HOLD, TritWire::POS);
        assert_eq!(s.to_i8(),  1);
        assert_eq!(c.to_i8(),  0);
    }

    /// TPUSH +1, TPUSH -1, TADD → 0 (no carry). Reg0 stays 0.
    #[test]
    fn test_rtl_add_pos_neg() {
        // TPUSH 0b10 (+1), TPUSH 0b01 (-1), TADD, THALT
        let code = vec![0x01, 0b10, 0x01, 0b01, 0x02, 0x00];
        let mut proc = BetRtlProcessor::new(code);
        let trace = proc.run(100);
        assert!(trace.halted, "should halt");
        assert_eq!(trace.final_stack, vec![0], "result should be 0");
        assert_eq!(trace.final_regs[0], 0);
    }

    /// TPUSH +1, TNEG → -1 on stack.
    #[test]
    fn test_rtl_neg() {
        let code = vec![0x01, 0b10, 0x04, 0x00]; // TPUSH +1, TNEG, THALT
        let mut proc = BetRtlProcessor::new(code);
        let trace = proc.run(100);
        assert!(trace.halted);
        assert_eq!(trace.final_stack, vec![-1]);
    }

    /// TPUSH +1, TSTORE reg0, TLOAD reg0 → back on stack.
    #[test]
    fn test_rtl_store_load() {
        let code = vec![
            0x01, 0b10,  // TPUSH +1
            0x08, 0x00,  // TSTORE reg0
            0x09, 0x00,  // TLOAD  reg0
            0x00,        // THALT
        ];
        let mut proc = BetRtlProcessor::new(code);
        let trace = proc.run(100);
        assert!(trace.halted);
        assert_eq!(trace.final_stack, vec![1]);
        assert_eq!(trace.final_regs[0], 1);
    }

    /// TJMPPOS: push +1, then TJMPPOS to the THALT, skipping a TPUSH -1.
    #[test]
    fn test_rtl_conditional_jump_taken() {
        // Addr 0: TPUSH +1            (2 bytes → addr 2)
        // Addr 2: TJMPPOS to addr 8   (3 bytes → addr 5)
        // Addr 5: TPUSH -1            (2 bytes → addr 7, skipped)
        // Addr 7: TPUSH 0             (2 bytes)
        // Addr 9: THALT
        // Jump target: addr 9
        let code = vec![
            0x01, 0b10,              // TPUSH +1
            0x05, 9, 0,              // TJMPPOS → 9
            0x01, 0b01,              // TPUSH -1 (skipped)
            0x01, 0b11,              // TPUSH 0  (skipped)
            0x00,                    // THALT
        ];
        let mut proc = BetRtlProcessor::new(code);
        let trace = proc.run(100);
        assert!(trace.halted);
        // Conditional jump pops the condition trit — stack is empty after taken jump.
        // The skipped TPUSH -1 / TPUSH 0 never execute.
        assert_eq!(trace.final_stack, vec![]);
    }

    /// TCONS: two agreeing trits → same value.
    #[test]
    fn test_rtl_cons() {
        let code = vec![
            0x01, 0b10,  // TPUSH +1
            0x01, 0b10,  // TPUSH +1
            0x0e,        // TCONS
            0x00,        // THALT
        ];
        let mut proc = BetRtlProcessor::new(code);
        let trace = proc.run(100);
        assert!(trace.halted);
        assert_eq!(trace.final_stack, vec![1]); // consensus(+1, +1) = +1
    }

    /// Consensus of conflicting trits → hold (deliberation).
    #[test]
    fn test_rtl_cons_conflict_to_hold() {
        let code = vec![
            0x01, 0b10,  // TPUSH +1
            0x01, 0b01,  // TPUSH -1
            0x0e,        // TCONS
            0x00,        // THALT
        ];
        let mut proc = BetRtlProcessor::new(code);
        let trace = proc.run(100);
        assert!(trace.halted);
        assert_eq!(trace.final_stack, vec![0]); // consensus(+1, -1) = 0 (hold)
    }

    /// Regfile initialises to hold (0), reset behaviour matches bet_regfile.v.
    #[test]
    fn test_regfile_reset() {
        let rf = BetRegfile::new();
        for i in 0..27u8 {
            assert_eq!(rf.read(i).to_i8(), 0, "reg {} should init to hold", i);
        }
    }

    /// trit_add carry chain: verify both overflow cases from the Verilog table.
    #[test]
    fn test_add_carry_chain() {
        // +1 + +1 = -1 carry+1 (sum wraps, carry positive)
        let (s, c) = trit_add(TritWire::POS, TritWire::POS);
        let (s2, c2) = trit_add(s, c); // (-1) + (+1) = 0, carry 0
        assert_eq!(s2.to_i8(), 0);
        assert_eq!(c2.to_i8(), 0);
    }
}
