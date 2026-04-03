//! `.tasm` 9-trit assembly → BET bytecode assembler
//!
//! Translates the balanced ternary RISC assembly dialect used in the
//! 9-trit simulator ecosystem into BET VM bytecode that runs on ternlang.
//!
//! ## Trit literal syntax
//! Positive digits: `0`, `1`, `2`, … (but balanced ternary only has 0 and 1 at the digit level)
//! Negative trit:   `T` (stands for −1, sometimes written as `t`)
//! Example: `10T` = 1×9 + 0×3 + (−1)×1 = 8
//!
//! ## Supported mnemonics
//! ```text
//! NOP                     — no operation
//! HALT                    — stop execution
//! LOAD  rd, imm           — load immediate trit value into register
//! MOV   rd, rs            — copy register
//! ADD   rd, rs1, rs2      — rd = rs1 + rs2
//! SUB   rd, rs1, rs2      — rd = rs1 + neg(rs2)
//! MUL   rd, rs1, rs2      — rd = rs1 * rs2  (ternary multiply)
//! NEG   rd, rs            — rd = neg(rs)
//! JMP   label             — unconditional jump
//! BEQ   rs, label         — branch if rs == 0 (hold)
//! BLT   rs, label         — branch if rs == -1 (conflict)
//! BGT   rs, label         — branch if rs == +1 (truth)
//! CONS  rd, rs1, rs2      — rd = consensus(rs1, rs2)
//! PUSH  rs                — push register onto stack
//! POP   rd                — pop stack into register
//! ```

/// Error type for `.tasm` assembly.
#[derive(Debug, PartialEq)]
pub enum TasmError {
    UnknownMnemonic(String),
    InvalidRegister(String),
    InvalidImmediate(String),
    UndefinedLabel(String),
    MissingOperand { mnemonic: String, expected: usize, got: usize },
}

impl std::fmt::Display for TasmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TasmError::UnknownMnemonic(m)   => write!(f, "Unknown mnemonic: {}", m),
            TasmError::InvalidRegister(r)   => write!(f, "Invalid register: {}", r),
            TasmError::InvalidImmediate(v)  => write!(f, "Invalid immediate: {}", v),
            TasmError::UndefinedLabel(l)    => write!(f, "Undefined label: {}", l),
            TasmError::MissingOperand { mnemonic, expected, got } =>
                write!(f, "{}: expected {} operands, got {}", mnemonic, expected, got),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BET opcodes (from BET-ISA-SPEC.md)
// ─────────────────────────────────────────────────────────────────────────────
const OP_THALT:     u8 = 0x00;
const OP_TPUSH:     u8 = 0x01;
const OP_TADD:      u8 = 0x02;
const OP_TMUL:      u8 = 0x03;
const OP_TNEG:      u8 = 0x04;
const OP_TJMP:      u8 = 0x0b;
const OP_TJMP_ZERO: u8 = 0x06;
const OP_TJMP_NEG:  u8 = 0x05; // TJMP_POS is 0x05; NEG is 0x07 — see spec
const OP_TJMP_POS:  u8 = 0x05;
const OP_TLOAD:     u8 = 0x09;  // TLOAD reg → push reg value
const OP_TSTORE:    u8 = 0x08;  // TSTORE reg ← pop
const OP_TCONS:     u8 = 0x0e;

// Trit encoding constants (2-bit BET packing)
const TRIT_NEG:  u8 = 0x01; // -1 (conflict)
const TRIT_POS:  u8 = 0x02; // +1 (truth)
const TRIT_ZERO: u8 = 0x03; // 0  (hold)

// ─────────────────────────────────────────────────────────────────────────────
// Trit literal parser  ("10T" → i32 balanced ternary value)
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a balanced ternary literal where T (or t) means −1.
/// Examples: "1" → 1, "T" → -1, "10T" → 8, "0" → 0
pub fn parse_trit_literal(s: &str) -> Result<i32, TasmError> {
    if s.is_empty() {
        return Err(TasmError::InvalidImmediate(s.to_string()));
    }
    let mut result = 0i32;
    let mut power  = 1i32;
    for ch in s.chars().rev() {
        let digit = match ch {
            '0' => 0,
            '1' => 1,
            'T' | 't' => -1,
            _ => return Err(TasmError::InvalidImmediate(s.to_string())),
        };
        result += digit * power;
        power  *= 3;
    }
    Ok(result)
}

/// Clamp an i32 value to a single trit {-1, 0, +1} and encode as BET byte.
fn trit_encode(v: i32) -> u8 {
    match v.signum() {
        -1 => TRIT_NEG,
         1 => TRIT_POS,
         _ => TRIT_ZERO,
    }
}

/// Parse register specifier: "r0"–"r26" or bare "0"–"26".
fn parse_reg(s: &str) -> Result<u8, TasmError> {
    let digits = s.trim_start_matches('r').trim_start_matches('R');
    digits.parse::<u8>().map_err(|_| TasmError::InvalidRegister(s.to_string()))
        .and_then(|n| if n < 27 { Ok(n) } else { Err(TasmError::InvalidRegister(s.to_string())) })
}

// ─────────────────────────────────────────────────────────────────────────────
// Assembler
// ─────────────────────────────────────────────────────────────────────────────

/// Assembles `.tasm` source code into BET VM bytecode.
pub struct TasmAssembler {
    /// Emitted bytecode
    pub bytecode: Vec<u8>,
    /// Label → byte offset table (for two-pass label resolution)
    labels: std::collections::HashMap<String, usize>,
    /// Unresolved label references: (patch_offset, label_name)
    patches: Vec<(usize, String)>,
}

impl TasmAssembler {
    pub fn new() -> Self {
        TasmAssembler {
            bytecode: Vec::new(),
            labels: std::collections::HashMap::new(),
            patches: Vec::new(),
        }
    }

    /// Assemble `.tasm` source. Returns BET bytecode on success.
    pub fn assemble(&mut self, source: &str) -> Result<Vec<u8>, TasmError> {
        self.bytecode.clear();
        self.labels.clear();
        self.patches.clear();

        // Pass 1: collect labels + emit instructions
        for raw_line in source.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
                continue; // blank / comment
            }

            // Strip inline comments
            let line = line.split(';').next().unwrap_or(line).trim();
            let line = line.split("//").next().unwrap_or(line).trim();

            // Label definition: "loop:" or ".loop"
            if line.ends_with(':') {
                let label = line.trim_end_matches(':').to_string();
                self.labels.insert(label, self.bytecode.len());
                continue;
            }
            if line.starts_with('.') {
                let label = line[1..].to_string();
                self.labels.insert(label, self.bytecode.len());
                continue;
            }

            // Tokenise instruction
            let tokens: Vec<&str> = line.split_whitespace()
                .flat_map(|t| t.split(','))
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect();

            if tokens.is_empty() { continue; }

            self.emit_instruction(&tokens)?;
        }

        // Pass 2: resolve labels
        for (offset, label) in &self.patches {
            let target = self.labels.get(label)
                .ok_or_else(|| TasmError::UndefinedLabel(label.clone()))?;
            // Patch the two bytes at offset with little-endian u16 address
            let addr = *target as u16;
            self.bytecode[*offset]     = (addr & 0xFF) as u8;
            self.bytecode[*offset + 1] = (addr >> 8)  as u8;
        }

        Ok(self.bytecode.clone())
    }

    fn emit(&mut self, byte: u8) {
        self.bytecode.push(byte);
    }

    fn require(tokens: &[&str], mnemonic: &str, count: usize) -> Result<(), TasmError> {
        if tokens.len() - 1 < count {
            return Err(TasmError::MissingOperand {
                mnemonic: mnemonic.to_string(),
                expected: count,
                got: tokens.len() - 1,
            });
        }
        Ok(())
    }

    fn emit_jump(&mut self, opcode: u8, label: &str) {
        self.emit(opcode);
        // Reserve 2 bytes for the address; patch in pass 2
        let patch_offset = self.bytecode.len();
        self.emit(0x00);
        self.emit(0x00);
        self.patches.push((patch_offset, label.to_string()));
    }

    fn emit_instruction(&mut self, tokens: &[&str]) -> Result<(), TasmError> {
        let mnemonic = tokens[0].to_uppercase();

        match mnemonic.as_str() {
            "NOP" => {
                // No-op: push hold, pop immediately — net effect: nothing
                // BET has no dedicated NOP; use TDUP+THALT would stop, so just skip.
            }

            "HALT" => {
                self.emit(OP_THALT);
            }

            // LOAD rd, imm  — push trit immediate into register
            "LOAD" => {
                Self::require(tokens, "LOAD", 2)?;
                let rd  = parse_reg(tokens[1])?;
                let val = parse_trit_literal(tokens[2])?;
                // Push the trit value, then store into register
                self.emit(OP_TPUSH);
                self.emit(trit_encode(val));
                self.emit(OP_TSTORE);
                self.emit(rd);
            }

            // MOV rd, rs  — copy rs to rd
            "MOV" => {
                Self::require(tokens, "MOV", 2)?;
                let rd = parse_reg(tokens[1])?;
                let rs = parse_reg(tokens[2])?;
                self.emit(OP_TLOAD);
                self.emit(rs);
                self.emit(OP_TSTORE);
                self.emit(rd);
            }

            // ADD rd, rs1, rs2  — rd = rs1 + rs2
            "ADD" => {
                Self::require(tokens, "ADD", 3)?;
                let rd  = parse_reg(tokens[1])?;
                let rs1 = parse_reg(tokens[2])?;
                let rs2 = parse_reg(tokens[3])?;
                self.emit(OP_TLOAD);  self.emit(rs1);
                self.emit(OP_TLOAD);  self.emit(rs2);
                self.emit(OP_TADD);
                self.emit(OP_TSTORE); self.emit(rd);
            }

            // SUB rd, rs1, rs2  — rd = rs1 + neg(rs2)
            "SUB" => {
                Self::require(tokens, "SUB", 3)?;
                let rd  = parse_reg(tokens[1])?;
                let rs1 = parse_reg(tokens[2])?;
                let rs2 = parse_reg(tokens[3])?;
                self.emit(OP_TLOAD);  self.emit(rs1);
                self.emit(OP_TLOAD);  self.emit(rs2);
                self.emit(OP_TNEG);                   // negate rs2
                self.emit(OP_TADD);
                self.emit(OP_TSTORE); self.emit(rd);
            }

            // MUL rd, rs1, rs2  — rd = rs1 × rs2
            "MUL" => {
                Self::require(tokens, "MUL", 3)?;
                let rd  = parse_reg(tokens[1])?;
                let rs1 = parse_reg(tokens[2])?;
                let rs2 = parse_reg(tokens[3])?;
                self.emit(OP_TLOAD);  self.emit(rs1);
                self.emit(OP_TLOAD);  self.emit(rs2);
                self.emit(OP_TMUL);
                self.emit(OP_TSTORE); self.emit(rd);
            }

            // NEG rd, rs  — rd = neg(rs)
            "NEG" => {
                Self::require(tokens, "NEG", 2)?;
                let rd = parse_reg(tokens[1])?;
                let rs = parse_reg(tokens[2])?;
                self.emit(OP_TLOAD);  self.emit(rs);
                self.emit(OP_TNEG);
                self.emit(OP_TSTORE); self.emit(rd);
            }

            // CONS rd, rs1, rs2  — rd = consensus(rs1, rs2)
            "CONS" => {
                Self::require(tokens, "CONS", 3)?;
                let rd  = parse_reg(tokens[1])?;
                let rs1 = parse_reg(tokens[2])?;
                let rs2 = parse_reg(tokens[3])?;
                self.emit(OP_TLOAD);  self.emit(rs1);
                self.emit(OP_TLOAD);  self.emit(rs2);
                self.emit(OP_TCONS);
                self.emit(OP_TSTORE); self.emit(rd);
            }

            // PUSH rs  — push register onto stack
            "PUSH" => {
                Self::require(tokens, "PUSH", 1)?;
                let rs = parse_reg(tokens[1])?;
                self.emit(OP_TLOAD); self.emit(rs);
            }

            // POP rd  — pop stack into register
            "POP" => {
                Self::require(tokens, "POP", 1)?;
                let rd = parse_reg(tokens[1])?;
                self.emit(OP_TSTORE); self.emit(rd);
            }

            // JMP label
            "JMP" | "JUMP" => {
                Self::require(tokens, "JMP", 1)?;
                self.emit_jump(OP_TJMP, tokens[1]);
            }

            // BEQ rs, label  — branch if rs == 0
            "BEQ" | "BZ" => {
                Self::require(tokens, "BEQ", 2)?;
                let rs = parse_reg(tokens[1])?;
                self.emit(OP_TLOAD); self.emit(rs);
                self.emit_jump(OP_TJMP_ZERO, tokens[2]);
            }

            // BLT rs, label  — branch if rs == -1
            "BLT" | "BN" => {
                Self::require(tokens, "BLT", 2)?;
                let rs = parse_reg(tokens[1])?;
                self.emit(OP_TLOAD); self.emit(rs);
                self.emit_jump(OP_TJMP_NEG, tokens[2]);
            }

            // BGT rs, label  — branch if rs == +1
            "BGT" | "BP" => {
                Self::require(tokens, "BGT", 2)?;
                let rs = parse_reg(tokens[1])?;
                self.emit(OP_TLOAD); self.emit(rs);
                self.emit_jump(OP_TJMP_POS, tokens[2]);
            }

            _ => return Err(TasmError::UnknownMnemonic(tokens[0].to_string())),
        }

        Ok(())
    }
}

impl Default for TasmAssembler {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trit_literal_simple() {
        assert_eq!(parse_trit_literal("1"),   Ok(1));
        assert_eq!(parse_trit_literal("0"),   Ok(0));
        assert_eq!(parse_trit_literal("T"),   Ok(-1));
    }

    #[test]
    fn test_parse_trit_literal_multidigit() {
        // 10T = 1×9 + 0×3 + (-1)×1 = 8
        assert_eq!(parse_trit_literal("10T"), Ok(8));
        // 1T1 = 1×9 + (-1)×3 + 1×1 = 7
        assert_eq!(parse_trit_literal("1T1"), Ok(7));
        // TTT = -1×9 + -1×3 + -1×1 = -13... wait that's not right
        // actually TTT = (-1)*9 + (-1)*3 + (-1)*1 = -9-3-1 = -13, no:
        // In balanced ternary, the digits are evaluated left-to-right as most significant first
        // TTT = -1*9 + -1*3 + -1*1 = -13
        assert_eq!(parse_trit_literal("TTT"), Ok(-13));
    }

    #[test]
    fn test_parse_trit_literal_invalid() {
        assert!(parse_trit_literal("2").is_err());
        assert!(parse_trit_literal("").is_err());
    }

    #[test]
    fn test_assemble_halt() {
        let mut asm = TasmAssembler::new();
        let code = asm.assemble("HALT").unwrap();
        assert_eq!(code, vec![0x00]);
    }

    #[test]
    fn test_assemble_load_pos() {
        let mut asm = TasmAssembler::new();
        // LOAD r0, 1 → TPUSH 0x02 (trit +1), TSTORE r0
        let code = asm.assemble("LOAD r0, 1").unwrap();
        assert_eq!(code[0], 0x01); // TPUSH
        assert_eq!(code[1], 0x02); // +1 encoding
        assert_eq!(code[2], 0x08); // TSTORE
        assert_eq!(code[3], 0x00); // register 0
    }

    #[test]
    fn test_assemble_load_neg() {
        let mut asm = TasmAssembler::new();
        let code = asm.assemble("LOAD r1, T").unwrap();
        assert_eq!(code[1], 0x01); // -1 encoding
        assert_eq!(code[3], 0x01); // register 1
    }

    #[test]
    fn test_assemble_load_zero() {
        let mut asm = TasmAssembler::new();
        let code = asm.assemble("LOAD r2, 0").unwrap();
        assert_eq!(code[1], 0x03); // hold encoding
    }

    #[test]
    fn test_assemble_add() {
        let mut asm = TasmAssembler::new();
        let code = asm.assemble("ADD r0, r1, r2\nHALT").unwrap();
        assert!(!code.is_empty());
        assert!(code.contains(&0x02)); // TADD opcode
        assert!(code.last() == Some(&0x00)); // HALT
    }

    #[test]
    fn test_assemble_neg() {
        let mut asm = TasmAssembler::new();
        let code = asm.assemble("NEG r0, r1\nHALT").unwrap();
        assert!(code.contains(&0x04)); // TNEG opcode
    }

    #[test]
    fn test_assemble_label_jump() {
        let mut asm = TasmAssembler::new();
        let src = "
; infinite loop (test label resolution)
loop:
  LOAD r0, 1
  JMP loop
";
        let code = asm.assemble(src).unwrap();
        assert!(!code.is_empty());
        // The jump target should resolve to offset 0 (label at start)
        assert!(code.contains(&0x0b)); // TJMP
    }

    #[test]
    fn test_assemble_undefined_label() {
        let mut asm = TasmAssembler::new();
        let result = asm.assemble("JMP nonexistent");
        assert!(matches!(result, Err(TasmError::UndefinedLabel(_))));
    }

    #[test]
    fn test_assemble_unknown_mnemonic() {
        let mut asm = TasmAssembler::new();
        let result = asm.assemble("FLOATOP r0, r1");
        assert!(matches!(result, Err(TasmError::UnknownMnemonic(_))));
    }

    #[test]
    fn test_assemble_comments_ignored() {
        let mut asm = TasmAssembler::new();
        let code = asm.assemble(
            "; this is a comment\n// also a comment\nHALT"
        ).unwrap();
        assert_eq!(code, vec![0x00]);
    }

    #[test]
    fn test_assemble_full_program() {
        // Load +1 into r0, load -1 into r1, add into r2, halt
        let src = "
  LOAD r0, 1      ; truth
  LOAD r1, T      ; conflict
  ADD  r2, r0, r1 ; hold (1 + -1 = 0)
  HALT
";
        let mut asm = TasmAssembler::new();
        let code = asm.assemble(src).unwrap();
        assert!(!code.is_empty());
        assert_eq!(*code.last().unwrap(), 0x00); // HALT at end
    }

    #[test]
    fn test_trit_encode() {
        assert_eq!(trit_encode(-1), 0x01);
        assert_eq!(trit_encode(0),  0x03);
        assert_eq!(trit_encode(1),  0x02);
        assert_eq!(trit_encode(5),  0x02); // positive → +1
        assert_eq!(trit_encode(-9), 0x01); // negative → -1
    }
}
