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
}

impl Default for Value {
    fn default() -> Self {
        Value::Trit(Trit::Zero)
    }
}

pub struct BetVm {
    registers: [Value; 27],
    carry_reg: Trit,
    stack: Vec<Value>,
    call_stack: Vec<usize>,  // Return addresses for TCALL/TRET
    tensors: Vec<Vec<Trit>>, // Simple heap for now
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
            pc: 0,
            code,
        }
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
                0x00 => return Ok(()), // Thalt
                _ => return Err(VmError::InvalidOpcode(opcode)),
            }
        }
        Ok(())
    }

    pub fn get_register(&self, reg: usize) -> Value {
        self.registers[reg]
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
