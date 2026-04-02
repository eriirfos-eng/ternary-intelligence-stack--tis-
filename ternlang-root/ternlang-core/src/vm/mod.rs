pub mod bet;

use crate::trit::Trit;
use crate::vm::bet::{unpack_trits, BetFault};

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum VmError {
    StackUnderflow,
    BetFault(BetFault),
    Halt,
    InvalidOpcode(u8),
    InvalidRegister(u8),
    PcOutOfBounds(usize),
    TypeMismatch,
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::StackUnderflow => write!(f, "Stack underflow"),
            VmError::BetFault(fault) => write!(f, "BET Fault: {:?}", fault),
            VmError::Halt => write!(f, "VM Halted"),
            VmError::InvalidOpcode(op) => write!(f, "Invalid opcode: 0x{:02x}", op),
            VmError::InvalidRegister(reg) => write!(f, "Invalid register: {}", reg),
            VmError::PcOutOfBounds(pc) => write!(f, "PC out of bounds: {}", pc),
            VmError::TypeMismatch => write!(f, "Type mismatch"),
        }
    }
}

#[repr(u8)]
pub enum Opcode {
    Tpush(Trit) = 0x01,
    Tadd = 0x02,
    Tmul = 0x03,
    Tneg = 0x04,
    TjmpPos(u16) = 0x05,
    TjmpZero(u16) = 0x06,
    TjmpNeg(u16) = 0x07,
    Tstore(u8) = 0x08,
    Tload(u8) = 0x09,
    Tdup = 0x0a,
    Tjmp(u16) = 0x0b,
    Tpop = 0x0c,
    TloadCarry = 0x0d,
    Tcons = 0x0e,
    Talloc(u16) = 0x0f,
    Tcall(u16) = 0x10,  // Call function at address, push return addr to call stack
    Tret = 0x11,         // Return: pop call stack, jump back
    Thalt = 0x00,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Trit(Trit),
    Int(i64),
    TensorRef(usize),
    AgentRef(usize),
}

impl Default for Value {
    fn default() -> Self {
        Value::Trit(Trit::Zero)
    }
}

/// A running agent instance.
/// v0.1: synchronous local actors — handler_addr is the bytecode address of the `handle` fn.
struct AgentInstance {
    handler_addr: usize,
    mailbox: std::collections::VecDeque<Value>,
}

pub struct BetVm {
    registers: [Value; 27],
    carry_reg: Trit,
    stack: Vec<Value>,
    call_stack: Vec<usize>,  // Return addresses for TCALL/TRET
    tensors: Vec<Vec<Trit>>, // Simple heap for now
    agents: Vec<AgentInstance>,
    /// agent_types[type_id] = handler_addr — registered at spawn time via TSPAWN
    agent_types: std::collections::HashMap<u16, usize>,
    pc: usize,
    code: Vec<u8>,
}

impl BetVm {
    pub fn new(code: Vec<u8>) -> Self {
        Self {
            registers: [Value::default(); 27],
            carry_reg: Trit::Zero,
            stack: Vec::new(),
            call_stack: Vec::new(),
            tensors: Vec::new(),
            agents: Vec::new(),
            agent_types: std::collections::HashMap::new(),
            pc: 0,
            code,
        }
    }

    /// Register an agent type (handler_addr) under a type_id.
    /// Called by the codegen runtime before spawning instances.
    pub fn register_agent_type(&mut self, type_id: u16, handler_addr: usize) {
        self.agent_types.insert(type_id, handler_addr);
    }

    pub fn run(&mut self) -> Result<(), VmError> {
        loop {
            if self.pc >= self.code.len() {
                break;
            }

            let opcode = self.code[self.pc];
            self.pc += 1;

            match opcode {
                0x01 => { // Tpush
                    if self.pc >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let packed = self.code[self.pc];
                    self.pc += 1;
                    let trits = unpack_trits(&[packed], 1).map_err(VmError::BetFault)?;
                    self.stack.push(Value::Trit(trits[0]));
                }
                0x02 => { // Tadd
                    let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (a, b) {
                        (Value::Trit(av), Value::Trit(bv)) => {
                            let (sum, carry) = av + bv;
                            self.stack.push(Value::Trit(sum));
                            self.carry_reg = carry;
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x03 => { // Tmul
                    let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (a, b) {
                        (Value::Trit(av), Value::Trit(bv)) => {
                            self.stack.push(Value::Trit(av * bv));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x04 => { // Tneg
                    let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match a {
                        Value::Trit(av) => self.stack.push(Value::Trit(-av)),
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x05 => { // TjmpPos
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if let Value::Trit(Trit::PosOne) = val {
                        self.pc = addr;
                    }
                }
                0x06 => { // TjmpZero
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if let Value::Trit(Trit::Zero) = val {
                        self.pc = addr;
                    }
                }
                0x07 => { // TjmpNeg
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if let Value::Trit(Trit::NegOne) = val {
                        self.pc = addr;
                    }
                }
                0x08 => { // Tstore
                    if self.pc >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let reg = self.code[self.pc];
                    self.pc += 1;
                    let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if (reg as usize) >= self.registers.len() {
                        return Err(VmError::InvalidRegister(reg));
                    }
                    self.registers[reg as usize] = val;
                }
                0x09 => { // Tload
                    if self.pc >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let reg = self.code[self.pc];
                    self.pc += 1;
                    if (reg as usize) >= self.registers.len() {
                        return Err(VmError::InvalidRegister(reg));
                    }
                    self.stack.push(self.registers[reg as usize]);
                }
                0x0a => { // Tdup
                    let val = self.stack.last().ok_or(VmError::StackUnderflow)?;
                    self.stack.push(*val);
                }
                0x0b => { // Tjmp
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc = addr;
                }
                0x0c => { // Tpop
                    self.stack.pop().ok_or(VmError::StackUnderflow)?;
                }
                0x0d => { // TloadCarry
                    self.stack.push(Value::Trit(self.carry_reg));
                }
                0x0e => { // Tcons (Consensus Addition)
                    let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (a, b) {
                        (Value::Trit(av), Value::Trit(bv)) => {
                            let (sum, _carry) = av + bv;
                            self.stack.push(Value::Trit(sum));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x0f => { // Talloc
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let size = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    let idx = self.tensors.len();
                    self.tensors.push(vec![Trit::Zero; size]);
                    self.stack.push(Value::TensorRef(idx));
                }
                0x20 => { // TMATMUL — (tensor_ref_a, tensor_ref_b) → tensor_ref_result
                    let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (a_val, b_val) {
                        (Value::TensorRef(a_idx), Value::TensorRef(b_idx)) => {
                            // tensors stored flat row-major; infer square dims from len
                            let a_len = self.tensors[a_idx].len();
                            let b_len = self.tensors[b_idx].len();
                            let a_dim = (a_len as f64).sqrt() as usize;
                            let b_dim = (b_len as f64).sqrt() as usize;
                            if a_dim * a_dim != a_len || b_dim * b_dim != b_len || a_dim != b_dim {
                                return Err(VmError::TypeMismatch);
                            }
                            let n = a_dim;
                            let mut result = vec![Trit::Zero; n * n];
                            for row in 0..n {
                                for col in 0..n {
                                    let mut acc = Trit::Zero;
                                    for k in 0..n {
                                        let (prod, _) = self.tensors[a_idx][row * n + k]
                                            + (self.tensors[a_idx][row * n + k]
                                                * self.tensors[b_idx][k * n + col]);
                                        let (sum, _) = acc + prod;
                                        acc = sum;
                                    }
                                    result[row * n + col] = acc;
                                }
                            }
                            let out_idx = self.tensors.len();
                            self.tensors.push(result);
                            self.stack.push(Value::TensorRef(out_idx));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x21 => { // TSPARSE_MATMUL — matmul skipping zero-state weights (flagship)
                    let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (a_val, b_val) {
                        (Value::TensorRef(a_idx), Value::TensorRef(b_idx)) => {
                            let a_len = self.tensors[a_idx].len();
                            let n = (a_len as f64).sqrt() as usize;
                            let mut result = vec![Trit::Zero; n * n];
                            let mut skipped: usize = 0;
                            for row in 0..n {
                                for col in 0..n {
                                    let mut acc = Trit::Zero;
                                    for k in 0..n {
                                        let weight = self.tensors[b_idx][k * n + col];
                                        // SPARSE SKIP: zero weights contribute nothing — skip entirely
                                        if weight == Trit::Zero {
                                            skipped += 1;
                                            continue;
                                        }
                                        let prod = self.tensors[a_idx][row * n + k] * weight;
                                        let (sum, _) = acc + prod;
                                        acc = sum;
                                    }
                                    result[row * n + col] = acc;
                                }
                            }
                            let out_idx = self.tensors.len();
                            self.tensors.push(result);
                            // Push result ref and skipped count for observability
                            self.stack.push(Value::TensorRef(out_idx));
                            self.stack.push(Value::Int(skipped as i64));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x22 => { // TIDX — (tensor_ref, row, col) → trit
                    let col_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let row_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let ref_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (ref_val, row_val, col_val) {
                        (Value::TensorRef(idx), Value::Int(row), Value::Int(col)) => {
                            let n = (self.tensors[idx].len() as f64).sqrt() as usize;
                            let pos = row as usize * n + col as usize;
                            self.stack.push(Value::Trit(self.tensors[idx][pos]));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x23 => { // TSET — (tensor_ref, row, col, trit_val) → stores in place
                    let trit_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let col_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let row_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let ref_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match (ref_val, row_val, col_val, trit_val) {
                        (Value::TensorRef(idx), Value::Int(row), Value::Int(col), Value::Trit(t)) => {
                            let n = (self.tensors[idx].len() as f64).sqrt() as usize;
                            let pos = row as usize * n + col as usize;
                            self.tensors[idx][pos] = t;
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x24 => { // TSHAPE — tensor_ref → pushes (Int(rows), Int(cols)) onto stack
                    let ref_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match ref_val {
                        Value::TensorRef(idx) => {
                            let len = self.tensors[idx].len();
                            let n = (len as f64).sqrt() as usize;
                            self.stack.push(Value::Int(n as i64)); // rows
                            self.stack.push(Value::Int(n as i64)); // cols
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x25 => { // TSPARSITY — tensor_ref → Int(zero_count) on stack
                    let ref_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match ref_val {
                        Value::TensorRef(idx) => {
                            let zeros = self.tensors[idx].iter()
                                .filter(|&&t| t == Trit::Zero)
                                .count();
                            self.stack.push(Value::Int(zeros as i64));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x10 => { // Tcall
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    self.call_stack.push(self.pc); // push return address
                    self.pc = addr;
                }
                0x11 => { // Tret
                    match self.call_stack.pop() {
                        Some(return_addr) => self.pc = return_addr,
                        None => return Ok(()), // top-level return = halt
                    }
                }
                // ── Actor opcodes ────────────────────────────────────────────────
                0x30 => { // TSPAWN type_id:u16 — create agent instance, push AgentRef
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let type_id = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]);
                    self.pc += 2;
                    let handler_addr = *self.agent_types.get(&type_id)
                        .ok_or(VmError::InvalidOpcode(0x30))?;
                    let instance_id = self.agents.len();
                    self.agents.push(AgentInstance {
                        handler_addr,
                        mailbox: std::collections::VecDeque::new(),
                    });
                    self.stack.push(Value::AgentRef(instance_id));
                }
                0x31 => { // TSEND — (AgentRef, message) → push to mailbox
                    let message = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let agent_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match agent_val {
                        Value::AgentRef(id) => {
                            self.agents[id].mailbox.push_back(message);
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x32 => { // TAWAIT — AgentRef → pop mailbox, call handler, push result
                    let agent_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match agent_val {
                        Value::AgentRef(id) => {
                            let message = self.agents[id].mailbox.pop_front()
                                .unwrap_or(Value::Trit(Trit::Zero)); // empty mailbox → hold
                            let handler_addr = self.agents[id].handler_addr;
                            // Push message as argument, then TCALL the handler.
                            self.stack.push(message);
                            self.call_stack.push(self.pc); // return to after TAWAIT
                            self.pc = handler_addr;
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                // ─────────────────────────────────────────────────────────────────

                0x00 => return Ok(()), // Thalt
                _ => return Err(VmError::InvalidOpcode(opcode)),
            }
        }
        Ok(())
    }

    pub fn get_register(&self, reg: usize) -> Value {
        self.registers[reg]
    }

    pub fn get_tensor(&self, idx: usize) -> Option<&Vec<Trit>> {
        self.tensors.get(idx)
    }

    pub fn peek_stack(&self) -> Option<Value> {
        self.stack.last().copied()
    }
}

#[cfg(test)]
mod tensor_tests {
    use super::*;
    use crate::vm::bet::pack_trits;

    fn push_trit(code: &mut Vec<u8>, t: Trit) {
        code.push(0x01);
        code.extend(pack_trits(&[t]));
    }

    fn talloc(code: &mut Vec<u8>, size: u16) {
        code.push(0x0f);
        code.extend_from_slice(&size.to_le_bytes());
    }

    #[test]
    fn test_tsparsity() {
        // Allocate a 2x2 tensor (4 trits), all Zero → sparsity = 4
        let mut code = Vec::new();
        talloc(&mut code, 4); // TALLOC 4 → TensorRef(0) on stack
        code.push(0x08); code.push(0x00); // TSTORE reg0
        code.push(0x09); code.push(0x00); // TLOAD reg0
        code.push(0x25); // TSPARSITY → Int(4)
        code.push(0x08); code.push(0x01); // TSTORE reg1
        code.push(0x00); // THALT

        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        assert_eq!(vm.get_register(1), Value::Int(4));
    }

    #[test]
    fn test_tshape() {
        // Allocate a 4-element (2x2) tensor, check shape returns 2, 2
        let mut code = Vec::new();
        talloc(&mut code, 4);
        code.push(0x08); code.push(0x00); // TSTORE reg0
        code.push(0x09); code.push(0x00); // TLOAD reg0
        code.push(0x24); // TSHAPE → Int(rows), Int(cols)
        code.push(0x08); code.push(0x02); // TSTORE cols → reg2
        code.push(0x08); code.push(0x01); // TSTORE rows → reg1
        code.push(0x00); // THALT

        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        assert_eq!(vm.get_register(1), Value::Int(2)); // rows
        assert_eq!(vm.get_register(2), Value::Int(2)); // cols
    }

    #[test]
    fn test_tsparse_matmul_skips_zeros() {
        // Two 1x1 all-zero tensors: TSPARSE_MATMUL should skip the one multiply
        // and produce a zero result with skipped_count = 1
        let mut code = Vec::new();
        talloc(&mut code, 1); // TensorRef(0) = A, all zeros
        code.push(0x08); code.push(0x00); // TSTORE reg0
        talloc(&mut code, 1); // TensorRef(1) = W, all zeros
        code.push(0x08); code.push(0x01); // TSTORE reg1

        code.push(0x09); code.push(0x00); // TLOAD A ref → stack
        code.push(0x09); code.push(0x01); // TLOAD W ref → stack
        code.push(0x21); // TSPARSE_MATMUL → pushes TensorRef(result), then Int(skipped)
        code.push(0x08); code.push(0x03); // TSTORE skipped_count → reg3
        code.push(0x08); code.push(0x02); // TSTORE result_ref → reg2
        code.push(0x00); // THALT

        let mut vm = BetVm::new(code);
        vm.run().unwrap();

        // W is all-zero so all 1 multiply was skipped
        assert_eq!(vm.get_register(3), Value::Int(1));
        // Result tensor should be zero
        let result_ref = match vm.get_register(2) {
            Value::TensorRef(i) => i,
            _ => panic!("expected TensorRef"),
        };
        assert_eq!(vm.get_tensor(result_ref).unwrap()[0], Trit::Zero);
    }
}

#[cfg(test)]
mod actor_tests {
    use super::*;
    use crate::vm::bet::pack_trits;

    /// Integration test: spawn an agent, send a trit message, await the reply.
    /// The handler is an identity function: handle(msg: trit) → msg.
    ///
    /// Bytecode layout:
    ///   [0x00] TJMP → entry_point          (skip over handler body)
    ///   [handler_addr]: TPUSH msg (arg) already on stack when called via TAWAIT
    ///                   TRET
    ///   [entry_point]: TSPAWN type_id=0    → AgentRef(0)
    ///                  TSTORE reg0
    ///                  TLOAD  reg0         → AgentRef(0)
    ///                  TPUSH PosOne        → message
    ///                  TSEND               → sends +1 to agent's mailbox
    ///                  TLOAD  reg0         → AgentRef(0)
    ///                  TAWAIT              → pops mailbox, calls handler, pushes result
    ///                  TSTORE reg1         → result (+1) in reg1
    ///                  THALT
    #[test]
    fn test_actor_spawn_send_await() {
        let mut code = Vec::new();

        // [0] TJMP over handler (3 bytes total, patch after)
        let jmp_patch = code.len() + 1;
        code.push(0x0b); // TJMP
        code.extend_from_slice(&[0u8, 0u8]);

        // [3] Handler: identity — the message is already on the stack when TAWAIT calls us.
        //     Just TRET — leaves the message as the return value on the stack.
        let handler_addr = code.len();
        code.push(0x11); // TRET

        // Patch the TJMP to land here
        let entry = code.len() as u16;
        let bytes = entry.to_le_bytes();
        code[jmp_patch] = bytes[0];
        code[jmp_patch + 1] = bytes[1];

        // [entry] TSPAWN type_id=0 → AgentRef(0) on stack
        code.push(0x30); code.extend_from_slice(&0u16.to_le_bytes());
        code.push(0x08); code.push(0x00); // TSTORE reg0

        // TLOAD reg0 → AgentRef, TPUSH PosOne → message, TSEND
        code.push(0x09); code.push(0x00); // TLOAD reg0
        code.push(0x01); code.extend(pack_trits(&[Trit::PosOne])); // TPUSH +1
        code.push(0x31); // TSEND

        // TLOAD reg0 → AgentRef, TAWAIT
        code.push(0x09); code.push(0x00); // TLOAD reg0
        code.push(0x32); // TAWAIT → calls handler with message on stack
        code.push(0x08); code.push(0x01); // TSTORE reg1
        code.push(0x00); // THALT

        let mut vm = BetVm::new(code);
        vm.register_agent_type(0, handler_addr);
        vm.run().unwrap();

        // The agent echoed +1 back → reg1 = PosOne
        assert_eq!(vm.get_register(1), Value::Trit(Trit::PosOne));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::bet::pack_trits;

    #[test]
    fn test_vm_addition() {
        // Tpush 1, Tpush 1, Tadd, Tstore 0, TloadCarry, Tstore 1, Thalt
        let mut code = vec![0x01];
        code.extend(pack_trits(&[Trit::PosOne]));
        code.push(0x01);
        code.extend(pack_trits(&[Trit::PosOne]));
        code.push(0x02); // Tadd
        code.push(0x08); // Tstore 0
        code.push(0x00);
        code.push(0x0d); // TloadCarry
        code.push(0x08); // Tstore 1
        code.push(0x01);
        code.push(0x00); // Thalt
        
        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        assert_eq!(vm.get_register(0), Value::Trit(Trit::NegOne)); // Sum
        assert_eq!(vm.get_register(1), Value::Trit(Trit::PosOne)); // Carry
    }
}
