pub mod bet;

use crate::trit::Trit;
use crate::vm::bet::{unpack_trits, BetFault};

use std::fmt;
use std::sync::Arc;

// ─── Remote transport trait ───────────────────────────────────────────────────

/// Abstracts the TCP layer so `ternlang-core` doesn't depend on `ternlang-runtime`.
/// Implement this trait on `TernNode` in `ternlang-runtime`, then inject via
/// `BetVm::set_remote(Arc<dyn RemoteTransport>)`.
pub trait RemoteTransport: Send + Sync {
    /// Send a trit (-1/0/+1) to the specified remote agent's mailbox (fire-and-forget).
    fn remote_send(&self, node_addr: &str, agent_id: usize, trit: i8) -> std::io::Result<()>;
    /// Request the remote agent to process its mailbox and return the result trit.
    fn remote_await(&self, node_addr: &str, agent_id: usize) -> std::io::Result<i8>;
}

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
            VmError::StackUnderflow =>
                write!(f, "[BET-001] Stack underflow — you tried to pop a truth that wasn't there."),
            VmError::BetFault(fault) =>
                write!(f, "[BET-002] BET encoding fault: {fault:?}. The 0b00 state is invalid — only -1, 0, +1 exist."),
            VmError::Halt =>
                write!(f, "[BET-003] VM halted cleanly. Execution reached the end."),
            VmError::InvalidOpcode(op) =>
                write!(f, "[BET-004] Unknown opcode 0x{op:02x} — the machine doesn't know this instruction. Conflict state."),
            VmError::InvalidRegister(reg) =>
                write!(f, "[BET-005] Register {reg} is out of range. The BET has 27 registers (0–26)."),
            VmError::PcOutOfBounds(pc) =>
                write!(f, "[BET-006] PC {pc} is out of bounds — you jumped outside the known universe. Recompile."),
            VmError::TypeMismatch =>
                write!(f, "[BET-007] Runtime type mismatch — a trit was expected but something else arrived."),
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
    TnodeId = 0x12,      // Phase 5.1: Push current node address to stack
    TpushStr = 0x13,     // Phase 5.1: Push string literal
    Tless    = 0x14,     // Integer less-than: pop b, pop a → push trit(a < b)
    Tgreater = 0x15,     // Integer greater-than: pop b, pop a → push trit(a > b)
    Thalt = 0x00,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Trit(Trit),
    Int(i64),
    String(String),
    TensorRef(usize),
    /// AgentRef { instance_id, node_addr }
    /// node_addr: None = local, Some("host:port") = remote
    AgentRef(usize, Option<String>),
}

impl Default for Value {
    fn default() -> Self {
        Value::Trit(Trit::Tend)
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
    /// Phase 5.1: The local node's address (returned by TNODEID)
    node_id: String,
    /// Phase 5.1: Optional remote transport for cross-node TSEND/TAWAIT
    remote: Option<Arc<dyn RemoteTransport>>,
}

impl BetVm {
    pub fn new(code: Vec<u8>) -> Self {
        Self {
            registers: std::array::from_fn(|_| Value::default()),
            carry_reg: Trit::Tend,
            stack: Vec::new(),
            call_stack: Vec::new(),
            tensors: Vec::new(),
            agents: Vec::new(),
            agent_types: std::collections::HashMap::new(),
            pc: 0,
            code,
            node_id: "127.0.0.1:7373".to_string(), // Default
            remote: None,
        }
    }

    pub fn set_node_id(&mut self, node_id: String) {
        self.node_id = node_id;
    }

    /// Inject a remote transport so TSEND/TAWAIT can cross node boundaries.
    pub fn set_remote(&mut self, transport: Arc<dyn RemoteTransport>) {
        self.remote = Some(transport);
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
                    if let Value::Trit(Trit::Affirm) = val {
                        self.pc = addr;
                    }
                }
                0x06 => { // TjmpZero
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if let Value::Trit(Trit::Tend) = val {
                        self.pc = addr;
                    }
                }
                0x07 => { // TjmpNeg
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let addr = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]) as usize;
                    self.pc += 2;
                    let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if let Value::Trit(Trit::Reject) = val {
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
                    self.stack.push(self.registers[reg as usize].clone());
                }
                0x0a => { // Tdup
                    let val = self.stack.last().ok_or(VmError::StackUnderflow)?;
                    self.stack.push(val.clone());
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
                    self.tensors.push(vec![Trit::Tend; size]);
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
                            let mut result = vec![Trit::Tend; n * n];
                            for row in 0..n {
                                for col in 0..n {
                                    let mut acc = Trit::Tend;
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
                            let mut result = vec![Trit::Tend; n * n];
                            let mut skipped: usize = 0;
                            for row in 0..n {
                                for col in 0..n {
                                    let mut acc = Trit::Tend;
                                    for k in 0..n {
                                        let weight = self.tensors[b_idx][k * n + col];
                                        // SPARSE SKIP: zero weights contribute nothing — skip entirely
                                        if weight == Trit::Tend {
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
                                .filter(|&&t| t == Trit::Tend)
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
                // ── Tensor compression opcodes ───────────────────────────────────
                0x26 => { // TCOMPRESS — TensorRef → TensorRef (run-length compressed)
                    // Run-length encoding of a sparse trit tensor.
                    // Format: alternating (count: u8, trit: encoded) pairs.
                    // Compressed tensor is stored as a new entry in self.tensors.
                    // Stack: tensor_ref → compressed_ref
                    let ref_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match ref_val {
                        Value::TensorRef(idx) => {
                            let src = &self.tensors[idx].clone();
                            let compressed = rle_compress(src);
                            let new_idx = self.tensors.len();
                            self.tensors.push(compressed);
                            self.stack.push(Value::TensorRef(new_idx));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x27 => { // TUNPACK — compressed TensorRef → TensorRef (restored)
                    // Decodes a run-length compressed tensor back to dense form.
                    // Stack: compressed_ref → restored_ref
                    let ref_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match ref_val {
                        Value::TensorRef(idx) => {
                            let src = &self.tensors[idx].clone();
                            let unpacked = rle_decompress(src);
                            let new_idx = self.tensors.len();
                            self.tensors.push(unpacked);
                            self.stack.push(Value::TensorRef(new_idx));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                // ── Actor opcodes ────────────────────────────────────────────────
                0x30 => { // TSPAWN type_id:u16 — create local agent instance, push AgentRef
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
                    self.stack.push(Value::AgentRef(instance_id, None));
                }
                0x33 => { // TREMOTE_SPAWN (addr:String, type_id:u16) -> AgentRef(id, Some(addr))
                    let addr_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    if self.pc + 1 >= self.code.len() { return Err(VmError::PcOutOfBounds(self.pc)); }
                    let type_id = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]);
                    self.pc += 2;
                    if let Value::String(addr) = addr_val {
                        // For v0.1: we don't actually trigger network spawn here.
                        // We just push the remote AgentRef. The runtime (TernNode)
                        // will handle the real network call when TSEND/TAWAIT is used.
                        self.stack.push(Value::AgentRef(type_id as usize, Some(addr)));
                    } else {
                        return Err(VmError::TypeMismatch);
                    }
                }
                0x31 => { // TSEND — (AgentRef, message) → push to mailbox
                    let message = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    let agent_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match agent_val {
                        Value::AgentRef(id, None) => {
                            self.agents[id].mailbox.push_back(message);
                        }
                        Value::AgentRef(id, Some(addr)) => {
                            // Phase 5.1: Remote TSEND via injected RemoteTransport.
                            if let Some(rt) = &self.remote {
                                let trit_i8 = match message {
                                    Value::Trit(Trit::Affirm) =>  1i8,
                                    Value::Trit(Trit::Reject) => -1i8,
                                    _                         =>  0i8,
                                };
                                rt.remote_send(&addr, id, trit_i8)
                                    .map_err(|_| VmError::TypeMismatch)?;
                            }
                            // If no transport configured: silent no-op (local-only mode).
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x32 => { // TAWAIT — AgentRef → pop mailbox, call handler, push result
                    let agent_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                    match agent_val {
                        Value::AgentRef(id, None) => {
                            let message = self.agents[id].mailbox.pop_front()
                                .unwrap_or(Value::Trit(Trit::Tend)); // empty mailbox → hold
                            let handler_addr = self.agents[id].handler_addr;
                            // Push message as argument, then TCALL the handler.
                            self.stack.push(message);
                            self.call_stack.push(self.pc); // return to after TAWAIT
                            self.pc = handler_addr;
                        }
                        Value::AgentRef(id, Some(addr)) => {
                            // Phase 5.1: Remote TAWAIT via injected RemoteTransport.
                            let result = if let Some(rt) = &self.remote {
                                rt.remote_await(&addr, id)
                                    .map(|v| match v {
                                        1  => Trit::Affirm,
                                        -1 => Trit::Reject,
                                        _  => Trit::Tend,
                                    })
                                    .unwrap_or(Trit::Tend)
                            } else {
                                Trit::Tend // hold: no transport configured
                            };
                            self.stack.push(Value::Trit(result));
                        }
                        _ => return Err(VmError::TypeMismatch),
                    }
                }
                0x12 => { // TNODEID — push local node address
                    self.stack.push(Value::String(self.node_id.clone()));
                }
                0x14 => { // TLESS — integer less-than: pop b, pop a → push trit(a < b)
                    let b = self.stack.pop().unwrap_or(Value::Int(0));
                    let a = self.stack.pop().unwrap_or(Value::Int(0));
                    let result = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => {
                            if x < y { Trit::Affirm } else if x == y { Trit::Tend } else { Trit::Reject }
                        }
                        _ => Trit::Tend,
                    };
                    self.stack.push(Value::Trit(result));
                }
                0x15 => { // TGREATER — integer greater-than: pop b, pop a → push trit(a > b)
                    let b = self.stack.pop().unwrap_or(Value::Int(0));
                    let a = self.stack.pop().unwrap_or(Value::Int(0));
                    let result = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => {
                            if x > y { Trit::Affirm } else if x == y { Trit::Tend } else { Trit::Reject }
                        }
                        _ => Trit::Tend,
                    };
                    self.stack.push(Value::Trit(result));
                }
                // ─────────────────────────────────────────────────────────────────

                0x00 => return Ok(()), // Thalt
                _ => return Err(VmError::InvalidOpcode(opcode)),
            }
        }
        Ok(())
    }

    pub fn get_register(&self, reg: usize) -> Value {
        self.registers[reg].clone()
    }

    pub fn get_tensor(&self, idx: usize) -> Option<&Vec<Trit>> {
        self.tensors.get(idx)
    }

    pub fn peek_stack(&self) -> Option<Value> {
        self.stack.last().cloned()
    }
}

// ─── Run-length compression helpers (used by TCOMPRESS / TUNPACK) ────────────

/// Run-length encode a trit slice.
/// Output format: pairs of (run_length: u8 encoded as Trit::Tend count trick,
/// but since Trit has 3 values we encode runs as sequences of a sentinel + count.
///
/// Actual encoding stored in the tensor heap as a flat Vec<Trit>:
///   [Trit value, count_high, count_low, Trit value, count_high, count_low, ...]
/// where count = count_high * 3 + count_low  (base-3, max run = 8 trits)
/// For simplicity max run length is 255, encoded as two trits (base 16 = 4 bits).
///
/// We use a simple scheme: store pairs (value_trit, length_trit_sequence)
/// terminated by a sentinel. Here we use a flat encoding:
///   even index → trit value, odd index → run length as Int packed as Trit
/// Since we can't store arbitrary ints as trits, we store the raw Vec<Trit>
/// with a header sentinel (NegOne) followed by (value, count) pairs where
/// count is clamped to a single trit (1–3). For longer runs we emit multiple pairs.
pub fn rle_compress(src: &[Trit]) -> Vec<Trit> {
    if src.is_empty() { return vec![]; }
    let mut out = Vec::new();
    // Header: NegOne sentinel marks this as a compressed tensor
    out.push(Trit::Reject);

    let mut i = 0;
    while i < src.len() {
        let val = src[i];
        let mut run = 1usize;
        while i + run < src.len() && src[i + run] == val && run < 255 {
            run += 1;
        }
        // Encode run as series of (val, count_trit) pairs.
        // count_trit: PosOne=1, Zero=2 (by convention), NegOne=3... not ideal.
        // Use a simpler scheme: emit val followed by run-length in unary trit pairs.
        // For portability just encode run as (run / 3) Zero-trits + (run % 3) marker.
        // Simplest correct scheme: emit val, then run count in base 3 (2 trits = max 8).
        // For runs > 8 we emit multiple pairs.
        let mut remaining = run;
        while remaining > 0 {
            let chunk = remaining.min(8); // max 8 per pair (2×3+2, max 2-digit base-3)
            out.push(val);
            out.push(int_to_trit((chunk / 3) as i8)); // high trit
            out.push(int_to_trit((chunk % 3) as i8)); // low trit
            remaining -= chunk;
        }
        i += run;
    }
    out
}

/// Decode a run-length encoded trit slice back to dense form.
pub fn rle_decompress(src: &[Trit]) -> Vec<Trit> {
    if src.is_empty() { return vec![]; }
    // Check header sentinel
    if src[0] != Trit::Reject { return src.to_vec(); } // not compressed
    let mut out = Vec::new();
    let mut i = 1;
    while i + 2 < src.len() {
        let val   = src[i];
        let hi    = trit_to_int(src[i + 1]) as usize;
        let lo    = trit_to_int(src[i + 2]) as usize;
        let count = hi * 3 + lo;
        for _ in 0..count.max(1) { out.push(val); }
        i += 3;
    }
    out
}

fn int_to_trit(v: i8) -> Trit {
    match v {
        0 => Trit::Tend,
        1 => Trit::Affirm,
        _ => Trit::Reject,
    }
}

fn trit_to_int(t: Trit) -> i8 {
    match t {
        Trit::Tend   => 0,
        Trit::Affirm => 1,
        Trit::Reject => 2, // used as digit '2' in base-3 run-length
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
        assert_eq!(vm.get_tensor(result_ref).unwrap()[0], Trit::Tend);
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
        code.push(0x01); code.extend(pack_trits(&[Trit::Affirm])); // TPUSH +1
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
        assert_eq!(vm.get_register(1), Value::Trit(Trit::Affirm));
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
        code.extend(pack_trits(&[Trit::Affirm]));
        code.push(0x01);
        code.extend(pack_trits(&[Trit::Affirm]));
        code.push(0x02); // Tadd
        code.push(0x08); // Tstore 0
        code.push(0x00);
        code.push(0x0d); // TloadCarry
        code.push(0x08); // Tstore 1
        code.push(0x01);
        code.push(0x00); // Thalt
        
        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        assert_eq!(vm.get_register(0), Value::Trit(Trit::Reject)); // Sum
        assert_eq!(vm.get_register(1), Value::Trit(Trit::Affirm)); // Carry
    }
}

#[cfg(test)]
mod compress_tests {
    use super::*;
    use crate::trit::Trit;

    #[test]
    fn test_rle_compress_all_zeros() {
        let src = vec![Trit::Tend; 9];
        let c = rle_compress(&src);
        // Must start with sentinel and be shorter than raw
        assert_eq!(c[0], Trit::Reject);
        assert!(c.len() < src.len(), "compressed should be shorter than 9 zeros");
    }

    #[test]
    fn test_rle_roundtrip_uniform() {
        let src = vec![Trit::Affirm; 6];
        let compressed = rle_compress(&src);
        let restored   = rle_decompress(&compressed);
        assert_eq!(restored, src, "roundtrip must be lossless");
    }

    #[test]
    fn test_rle_roundtrip_mixed() {
        let src = vec![
            Trit::Affirm, Trit::Affirm, Trit::Affirm,
            Trit::Tend,   Trit::Tend,
            Trit::Reject,
            Trit::Tend,   Trit::Tend,   Trit::Tend,
        ];
        let compressed = rle_compress(&src);
        let restored   = rle_decompress(&compressed);
        assert_eq!(restored, src, "roundtrip must be lossless for mixed tensor");
    }

    #[test]
    fn test_rle_compress_single_element() {
        let src = vec![Trit::Reject];
        let c = rle_compress(&src);
        let r = rle_decompress(&c);
        assert_eq!(r, src);
    }

    #[test]
    fn test_tcompress_tunpack_opcodes() {
        // Test TCOMPRESS (0x26) and TUNPACK (0x27) via VM bytecode.
        // Strategy: TALLOC a sparse tensor, compress it, unpack it,
        // check TSPARSITY is preserved. Use a pre-filled tensor (all zeros = maximum sparsity).
        let mut code = Vec::new();

        // TALLOC 9 elements (all zero by default)
        code.push(0x0f);
        code.extend_from_slice(&9u16.to_le_bytes());
        code.push(0x08); code.push(0x00); // TSTORE r0

        // TCOMPRESS r0 → compressed ref in r1
        code.push(0x09); code.push(0x00); // TLOAD r0
        code.push(0x26);                   // TCOMPRESS
        code.push(0x08); code.push(0x01); // TSTORE r1

        // TUNPACK r1 → restored ref in r2
        code.push(0x09); code.push(0x01); // TLOAD r1
        code.push(0x27);                   // TUNPACK
        code.push(0x08); code.push(0x02); // TSTORE r2

        // TSPARSITY on restored tensor → should be 9 (all zeros)
        code.push(0x09); code.push(0x02); // TLOAD r2
        code.push(0x25);                   // TSPARSITY
        code.push(0x08); code.push(0x03); // TSTORE r3

        code.push(0x00); // THALT

        let mut vm = BetVm::new(code);
        vm.run().unwrap();

        // All 9 elements should still be zero after compress→unpack
        let sparsity = vm.get_register(3);
        assert!(matches!(sparsity, Value::Int(n) if n >= 9),
            "restored tensor should have 9 zero elements, got {:?}", sparsity);
    }
}
