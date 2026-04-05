use crate::ast::*;
use crate::vm::bet::pack_trits;
use crate::trit::Trit;

pub struct BytecodeEmitter {
    code: Vec<u8>,
    symbols: std::collections::HashMap<String, u8>,
    func_addrs: std::collections::HashMap<String, u16>,
    break_patches: Vec<usize>, // addresses to patch when a loop ends
    next_reg: u8,
    /// Struct layouts: struct_name → ordered field names
    struct_layouts: std::collections::HashMap<String, Vec<String>>,
    /// Agent type IDs: agent_name → type_id (u16 index)
    agent_type_ids: std::collections::HashMap<String, u16>,
    /// Agent handler addresses emitted during emit_program (type_id → addr)
    agent_handlers: Vec<(u16, u16)>,
}

impl BytecodeEmitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            symbols: std::collections::HashMap::new(),
            func_addrs: std::collections::HashMap::new(),
            break_patches: Vec::new(),
            next_reg: 0,
            struct_layouts: std::collections::HashMap::new(),
            agent_type_ids: std::collections::HashMap::new(),
            agent_handlers: Vec::new(),
        }
    }

    /// After emit_program, call this to wire agent handler addresses into a VM.
    pub fn register_agents(&self, vm: &mut crate::vm::BetVm) {
        for &(type_id, addr) in &self.agent_handlers {
            vm.register_agent_type(type_id, addr as usize);
        }
    }

    pub fn emit_program(&mut self, program: &Program) {
        // Register struct layouts so field-access codegen knows field order.
        for s in &program.structs {
            let field_names: Vec<String> = s.fields.iter().map(|(n, _)| n.clone()).collect();
            self.struct_layouts.insert(s.name.clone(), field_names);
        }
        // Register agent type IDs before emitting bodies.
        for (idx, agent) in program.agents.iter().enumerate() {
            self.agent_type_ids.insert(agent.name.clone(), idx as u16);
        }

        // Two-pass: first emit a TJMP over all function/agent bodies.
        let entry_jmp_patch = self.code.len() + 1;
        self.code.push(0x0b); // TJMP — skip over function bodies
        self.code.extend_from_slice(&[0u8, 0u8]);

        // Emit agent handler methods (the `handle` fn of each agent).
        for agent in &program.agents {
            let type_id = self.agent_type_ids[&agent.name];
            // The first method named "handle" is the entry point.
            // All methods are emitted; the first one becomes the handler addr.
            let mut handler_addr: Option<u16> = None;
            for method in &agent.methods {
                let addr = self.code.len() as u16;
                if handler_addr.is_none() {
                    handler_addr = Some(addr);
                }
                self.emit_function(method);
                // Also register under the fully-qualified name "AgentName::method"
                let fq = format!("{}::{}", agent.name, method.name);
                self.func_addrs.insert(fq, addr);
            }
            if let Some(addr) = handler_addr {
                self.agent_handlers.push((type_id, addr));
            }
        }

        // Emit regular function bodies.
        for func in &program.functions {
            self.emit_function(func);
        }

        // Patch entry jump to land after all bodies.
        let after_funcs = self.code.len() as u16;
        self.patch_u16(entry_jmp_patch, after_funcs);
    }

    pub fn emit_function(&mut self, func: &Function) {
        // Record address of this function's first instruction.
        let func_addr = self.code.len() as u16;
        self.func_addrs.insert(func.name.clone(), func_addr);

        for stmt in &func.body {
            self.emit_stmt(stmt);
        }
        // Emit TRET at end of every function body.
        self.code.push(0x11); // TRET
    }

    pub fn emit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, ty, value } => {
                match ty {
                    Type::TritTensor { dims } => {
                        let size: usize = dims.iter().product();
                        self.code.push(0x0f); // TALLOC
                        self.code.extend_from_slice(&(size as u16).to_le_bytes());
                        let reg = self.next_reg;
                        self.symbols.insert(name.clone(), reg);
                        self.next_reg += 1;
                        self.code.push(0x08); // TSTORE
                        self.code.push(reg);
                    }
                    Type::Named(struct_name) => {
                        // Allocate one register per field, zero-initialised.
                        // Fields stored as "<instance>.<field>" in the symbol table.
                        let fields = self.struct_layouts.get(struct_name)
                            .cloned()
                            .unwrap_or_default();
                        // Record base register under the instance name too (not strictly needed).
                        let base_reg = self.next_reg;
                        self.symbols.insert(name.clone(), base_reg);
                        for field in &fields {
                            let reg = self.next_reg;
                            self.next_reg += 1;
                            self.symbols.insert(format!("{}.{}", name, field), reg);
                            // Zero-initialise each field
                            self.code.push(0x01); // TPUSH hold
                            self.code.extend(crate::vm::bet::pack_trits(&[crate::trit::Trit::Tend]));
                            self.code.push(0x08); // TSTORE
                            self.code.push(reg);
                        }
                        // If no fields, still emit the base placeholder
                        if fields.is_empty() {
                            self.next_reg += 1;
                            self.code.push(0x01);
                            self.code.extend(crate::vm::bet::pack_trits(&[crate::trit::Trit::Tend]));
                            self.code.push(0x08);
                            self.code.push(base_reg);
                        }
                    }
                    _ => {
                        self.emit_expr(value);
                        let reg = self.next_reg;
                        self.symbols.insert(name.clone(), reg);
                        self.next_reg += 1;
                        self.code.push(0x08); // TSTORE
                        self.code.push(reg);
                    }
                }
            }
            Stmt::FieldSet { object, field, value } => {
                // Resolve the mangled register name for this field.
                let key = format!("{}.{}", object, field);
                self.emit_expr(value);
                if let Some(&reg) = self.symbols.get(&key) {
                    self.code.push(0x08); // TSTORE
                    self.code.push(reg);
                }
                // Unknown field — emit nothing (will be a runtime no-op).
            }
            Stmt::IndexSet { object, row, col, value } => {
                if let Some(&reg) = self.symbols.get(object) {
                    self.code.push(0x09); self.code.push(reg); // TLOAD tensor ref
                    self.emit_expr(row);
                    self.emit_expr(col);
                    self.emit_expr(value);
                    self.code.push(0x23); // TSET
                }
            }
            Stmt::IfTernary { condition, on_pos, on_zero, on_neg } => {
                self.emit_expr(condition);
                
                // Jump to POS branch
                self.code.push(0x0a); // TDUP
                let jmp_pos_patch = self.code.len() + 1;
                self.code.push(0x05); // TJMP_POS
                self.code.extend_from_slice(&[0, 0]);

                // Jump to ZERO branch
                self.code.push(0x0a); // TDUP
                let jmp_zero_patch = self.code.len() + 1;
                self.code.push(0x06); // TJMP_ZERO
                self.code.extend_from_slice(&[0, 0]);

                // Fallthrough to NEG branch
                self.code.push(0x0c); // TPOP
                self.emit_stmt(on_neg);
                let end_jmp_neg_patch = self.code.len() + 1;
                self.code.push(0x0b); // TJMP
                self.code.extend_from_slice(&[0, 0]);

                // POS Branch
                let pos_addr = self.code.len() as u16;
                self.patch_u16(jmp_pos_patch, pos_addr);
                self.code.push(0x0c); // TPOP
                self.emit_stmt(on_pos);
                let end_jmp_pos_patch = self.code.len() + 1;
                self.code.push(0x0b); // TJMP
                self.code.extend_from_slice(&[0, 0]);

                // ZERO Branch
                let zero_addr = self.code.len() as u16;
                self.patch_u16(jmp_zero_patch, zero_addr);
                self.code.push(0x0c); // TPOP
                self.emit_stmt(on_zero);
                
                // End Label
                let end_addr = self.code.len() as u16;
                self.patch_u16(end_jmp_neg_patch, end_addr);
                self.patch_u16(end_jmp_pos_patch, end_addr);
            }
            Stmt::Match { condition, arms } => {
                self.emit_expr(condition);
                
                let mut patches = Vec::new();
                let mut end_patches = Vec::new();

                for (val, _stmt) in arms {
                    self.code.push(0x0a); // TDUP
                    let patch_pos = self.code.len() + 1;
                    match val {
                        1  => self.code.push(0x05), // TJMP_POS
                        0  => self.code.push(0x06), // TJMP_ZERO
                        -1 => self.code.push(0x07), // TJMP_NEG
                        _  => unreachable!(),
                    }
                    self.code.extend_from_slice(&[0, 0]);
                    patches.push((patch_pos, *val));
                }

                self.code.push(0x0c); // TPOP (If no match found)
                let end_jmp_no_match = self.code.len() + 1;
                self.code.push(0x0b); // TJMP
                self.code.extend_from_slice(&[0, 0]);

                for (patch_pos, val) in patches {
                    let addr = self.code.len() as u16;
                    self.patch_u16(patch_pos, addr);
                    self.code.push(0x0c); // TPOP
                    
                    // Find the stmt for this val
                    let stmt = arms.iter().find(|(v, _)| *v == val).unwrap().1.clone();
                    self.emit_stmt(&stmt);
                    
                    let end_patch = self.code.len() + 1;
                    self.code.push(0x0b); // TJMP
                    self.code.extend_from_slice(&[0, 0]);
                    end_patches.push(end_patch);
                }

                let end_addr = self.code.len() as u16;
                self.patch_u16(end_jmp_no_match, end_addr);
                for p in end_patches {
                    self.patch_u16(p, end_addr);
                }
            }
            // for <var> in <tensor_ref> { body }
            // Iterates over each trit in a tensor sequentially.
            Stmt::ForIn { var, iter, body } => {
                // Emit the iterable expression — expects a TensorRef on stack
                self.emit_expr(iter);
                let iter_reg = self.next_reg;
                self.symbols.insert(format!("__iter_{}", var), iter_reg);
                self.next_reg += 1;
                self.code.push(0x08); self.code.push(iter_reg); // TSTORE iter ref

                // Index register
                let idx_reg = self.next_reg;
                self.next_reg += 1;
                // TSHAPE → push (rows, cols); store cols for bound check
                self.code.push(0x09); self.code.push(iter_reg); // TLOAD tensor ref
                self.code.push(0x24); // TSHAPE → rows, cols
                let bound_reg = self.next_reg; self.next_reg += 1;
                self.code.push(0x08); self.code.push(bound_reg); // TSTORE cols (bound)
                // discard rows
                self.code.push(0x0c); // TPOP rows

                // Initialise index to 0 (hold trit — used as int here)
                self.code.push(0x01);
                self.code.extend(pack_trits(&[Trit::Tend]));
                self.code.push(0x08); self.code.push(idx_reg); // TSTORE idx=0

                let loop_top = self.code.len() as u16;

                // Load current element: TIDX(tensor, 0, idx) — simplified 1D walk
                self.code.push(0x09); self.code.push(iter_reg); // TLOAD tensor
                self.code.push(0x01); self.code.extend(pack_trits(&[Trit::Tend])); // row 0
                self.code.push(0x09); self.code.push(idx_reg); // TLOAD idx
                self.code.push(0x22); // TIDX → trit element
                let var_reg = self.next_reg; self.next_reg += 1;
                self.symbols.insert(var.clone(), var_reg);
                self.code.push(0x08); self.code.push(var_reg); // TSTORE var

                // Emit body
                self.emit_stmt(body);

                // Unconditional jump back to loop top
                let jmp_back = self.code.len() + 1;
                self.code.push(0x0b);
                self.code.extend_from_slice(&[0, 0]);
                self.patch_u16(jmp_back, loop_top);
            }

            // loop { body } — infinite loop, exited by break
            Stmt::Loop { body } => {
                let loop_top = self.code.len() as u16;
                // Track break patch sites
                let pre_break_count = self.break_patches.len();
                self.emit_stmt(body);
                // Jump back to top
                let jmp_back = self.code.len() + 1;
                self.code.push(0x0b);
                self.code.extend_from_slice(&[0, 0]);
                self.patch_u16(jmp_back, loop_top);
                // Collect break patches, then apply (avoids double borrow)
                let after_loop = self.code.len() as u16;
                let patches: Vec<usize> = self.break_patches.drain(pre_break_count..).collect();
                for patch in patches {
                    self.patch_u16(patch, after_loop);
                }
            }

            Stmt::Break => {
                let patch = self.code.len() + 1;
                self.code.push(0x0b); // TJMP (address patched by enclosing loop)
                self.code.extend_from_slice(&[0, 0]);
                self.break_patches.push(patch);
            }

            Stmt::Continue => {
                // Continue is a TJMP to loop top — needs enclosing loop context.
                // Emit as no-op for now (safe: loop naturally continues).
            }

            Stmt::Use { .. } => {
                // Module resolution not yet implemented — no-op at codegen level.
            }
            Stmt::Send { target, message } => {
                // Push AgentRef, then message, then emit TSEND (0x31).
                self.emit_expr(target);
                self.emit_expr(message);
                self.code.push(0x31); // TSEND
            }

            Stmt::WhileTernary { condition, on_pos, on_zero, on_neg } => {
                let loop_top = self.code.len() as u16;
                self.emit_expr(condition);
                // Ternary branch: pos → on_pos body, zero → on_zero body, neg → break
                self.code.push(0x0a); // TDUP
                let jmp_pos_patch = self.code.len() + 1;
                self.code.push(0x05); self.code.extend_from_slice(&[0, 0]); // TJMP_POS
                self.code.push(0x0a); // TDUP
                let jmp_zero_patch = self.code.len() + 1;
                self.code.push(0x06); self.code.extend_from_slice(&[0, 0]); // TJMP_ZERO
                // Neg branch: exit loop
                self.code.push(0x0c); // TPOP
                self.emit_stmt(on_neg);
                let exit_patch = self.code.len() + 1;
                self.code.push(0x0b); self.code.extend_from_slice(&[0, 0]); // TJMP exit

                // Pos branch
                let pos_addr = self.code.len() as u16;
                self.patch_u16(jmp_pos_patch, pos_addr);
                self.code.push(0x0c); // TPOP
                self.emit_stmt(on_pos);
                let back_pos = self.code.len() + 1;
                self.code.push(0x0b); self.code.extend_from_slice(&[0, 0]);
                self.patch_u16(back_pos, loop_top);

                // Zero branch
                let zero_addr = self.code.len() as u16;
                self.patch_u16(jmp_zero_patch, zero_addr);
                self.code.push(0x0c); // TPOP
                self.emit_stmt(on_zero);
                let back_zero = self.code.len() + 1;
                self.code.push(0x0b); self.code.extend_from_slice(&[0, 0]);
                self.patch_u16(back_zero, loop_top);

                // Exit label
                let exit_addr = self.code.len() as u16;
                self.patch_u16(exit_patch, exit_addr);
            }

            Stmt::Return(expr) => {
                self.emit_expr(expr);
                self.code.push(0x00); // THALT
            }
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
            }
            Stmt::Decorated { directive, stmt } => {
                if directive == "sparseskip" {
                    // Case 1: @sparseskip on a bare expression: matmul(a, b);
                    if let Stmt::Expr(inner_expr) = stmt.as_ref() {
                        if let Expr::Call { callee, args } = inner_expr {
                            if callee == "matmul" && args.len() == 2 {
                                self.emit_expr(&args[0]);
                                self.emit_expr(&args[1]);
                                self.code.push(0x21); // TSPARSE_MATMUL
                                return;
                            }
                        }
                    }
                    // Case 2: @sparseskip on a let binding: let c: trittensor = matmul(a, b);
                    if let Stmt::Let { name, value, .. } = stmt.as_ref() {
                        if let Expr::Call { callee, args } = value {
                            if callee == "matmul" && args.len() == 2 {
                                self.emit_expr(&args[0]);
                                self.emit_expr(&args[1]);
                                self.code.push(0x21); // TSPARSE_MATMUL
                                // TSPARSE_MATMUL pushes TensorRef then Int(skipped_count)
                                self.code.push(0x0c); // TPOP — discard skipped_count
                                let reg = self.next_reg;
                                self.symbols.insert(name.clone(), reg);
                                self.next_reg += 1;
                                self.code.push(0x08); // TSTORE tensor ref into register
                                self.code.push(reg);
                                return;
                            }
                        }
                    }
                }
                // Fallthrough: emit the inner statement unchanged
                self.emit_stmt(stmt);
            }
            _ => {}
        }
    }

    fn emit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::TritLiteral(val) => {
                self.code.push(0x01); // TPUSH
                let trit = Trit::from(*val);
                self.code.extend(pack_trits(&[trit]));
            }
            Expr::Ident(name) => {
                if let Some(&reg) = self.symbols.get(name) {
                    self.code.push(0x09); // TLOAD
                    self.code.push(reg);
                }
            }
            Expr::BinaryOp { op, lhs, rhs } => {
                self.emit_expr(lhs);
                self.emit_expr(rhs);
                match op {
                    BinOp::Add      => self.code.push(0x02), // TADD
                    BinOp::Mul      => self.code.push(0x03), // TMUL
                    BinOp::Sub      => { self.code.push(0x04); self.code.push(0x02); } // TNEG rhs, TADD
                    BinOp::Equal    => self.code.push(0x0e), // TCONS (equality via consensus)
                    BinOp::NotEqual => { self.code.push(0x0e); self.code.push(0x04); } // TCONS then TNEG
                    BinOp::And      => self.code.push(0x03), // TMUL (ternary AND = multiply)
                    BinOp::Or       => self.code.push(0x0e), // TCONS (ternary OR = consensus)
                    BinOp::Less     => self.code.push(0x14), // TLESS  (a < b → affirm/tend/reject)
                    BinOp::Greater  => self.code.push(0x15), // TGREATER (a > b → affirm/tend/reject)
                }
            }
            Expr::UnaryOp { op, expr } => {
                self.emit_expr(expr);
                match op {
                    UnOp::Neg => self.code.push(0x04),
                }
            }
            Expr::Call { callee, args } => {
                for arg in args {
                    self.emit_expr(arg);
                }
                match callee.as_str() {
                    "consensus" => {
                        if args.len() == 2 {
                            self.code.push(0x0e); // TCONS
                        }
                    }
                    "invert" => {
                        if args.len() == 1 {
                            self.code.push(0x04); // TNEG
                        }
                    }
                    "truth" => {
                        self.code.push(0x01); // TPUSH
                        self.code.extend(pack_trits(&[Trit::Affirm]));
                    }
                    "hold" => {
                        self.code.push(0x01); // TPUSH
                        self.code.extend(pack_trits(&[Trit::Tend]));
                    }
                    "conflict" => {
                        self.code.push(0x01); // TPUSH
                        self.code.extend(pack_trits(&[Trit::Reject]));
                    }
                    "matmul" => {
                        if args.len() == 2 {
                            self.code.push(0x20); // TMATMUL (dense)
                        }
                    }
                    "sparsity" => {
                        if args.len() == 1 {
                            self.code.push(0x25); // TSPARSITY
                        }
                    }
                    "shape" => {
                        if args.len() == 1 {
                            self.code.push(0x24); // TSHAPE
                        }
                    }
                    _ => {
                        // User-defined function call — emit TCALL if address known
                        if let Some(&addr) = self.func_addrs.get(callee) {
                            self.code.push(0x10); // TCALL
                            self.code.extend_from_slice(&addr.to_le_bytes());
                        } else {
                            // Forward reference: emit TCALL with placeholder, needs second pass
                            // For now emit hold as safe default
                            self.code.push(0x01); // TPUSH hold
                            self.code.extend(pack_trits(&[Trit::Tend]));
                        }
                    }
                }
            }
            Expr::FieldAccess { object, field } => {
                // Resolve to mangled register: "<object_name>.<field>"
                // Only handles single-level ident.field — nested access falls back to hold.
                if let Expr::Ident(obj_name) = object.as_ref() {
                    let key = format!("{}.{}", obj_name, field);
                    if let Some(&reg) = self.symbols.get(&key) {
                        self.code.push(0x09); // TLOAD
                        self.code.push(reg);
                        return;
                    }
                }
                // Fallback: push hold
                self.code.push(0x01);
                self.code.extend(pack_trits(&[Trit::Tend]));
            }
            Expr::Index { object, row, col } => {
                self.emit_expr(object);
                self.emit_expr(row);
                self.emit_expr(col);
                self.code.push(0x22); // TIDX
            }
            Expr::Propagate { expr } => {
                // Evaluate inner expression → stack: [val]
                self.emit_expr(expr);
                // TDUP → [val, dup]
                self.code.push(0x0a);
                // TJMP_NEG to propagate path — consumes dup; if -1 jumps, else [val] remains
                let neg_patch = self.code.len() + 1;
                self.code.push(0x07); // TJMP_NEG
                self.code.extend_from_slice(&[0u8, 0u8]);
                // Not -1: skip over the early return
                let skip_patch = self.code.len() + 1;
                self.code.push(0x0b); // TJMP
                self.code.extend_from_slice(&[0u8, 0u8]);
                // propagate path: val=-1 is still on stack — TRET returns it
                let prop_addr = self.code.len() as u16;
                self.patch_u16(neg_patch, prop_addr);
                self.code.push(0x11); // TRET
                // skip label: continue with val on stack
                let skip_addr = self.code.len() as u16;
                self.patch_u16(skip_patch, skip_addr);
            }
            Expr::Cast { expr, .. } => {
                // cast() is a no-op at the BET level — trits are already in canonical form.
                // Emit the inner expression; the type annotation guides the type checker only.
                self.emit_expr(expr);
            }
            Expr::Spawn { agent_name, node_addr } => {
                if let Some(addr) = node_addr {
                    // Remote spawn: push addr string, then TREMOTE_SPAWN(0x33)
                    self.emit_expr(&Expr::StringLiteral(addr.clone()));
                    if let Some(&type_id) = self.agent_type_ids.get(agent_name) {
                        self.code.push(0x33); // TREMOTE_SPAWN
                        self.code.extend_from_slice(&type_id.to_le_bytes());
                    } else {
                        self.code.push(0x01);
                        self.code.extend(pack_trits(&[Trit::Tend]));
                    }
                } else if let Some(&type_id) = self.agent_type_ids.get(agent_name) {
                    // Local spawn
                    self.code.push(0x30); // TSPAWN
                    self.code.extend_from_slice(&type_id.to_le_bytes());
                } else {
                    // Unknown agent — push hold as fallback
                    self.code.push(0x01);
                    self.code.extend(pack_trits(&[Trit::Tend]));
                }
            }
            Expr::StringLiteral(s) => {
                // For v0.1: we don't have a TPUSH_STRING opcode.
                // Instead, we hack it by passing strings out-of-band or
                // just ignoring them in the BET bytecode for now.
                // Actually, for remote spawn to work, we need to pass the address.
                // Let's assume the VM can handle a raw string in the value stack if pushed via a hook.
                // For now, emit a placeholder.
                self.code.push(0x01);
                self.code.extend(pack_trits(&[Trit::Tend]));
            }
            Expr::NodeId => {
                self.code.push(0x12); // TNODEID
            }
            Expr::Await { target } => {
                // Emit the AgentRef expression, then TAWAIT (0x32).
                // TAWAIT pops the AgentRef, pops its mailbox front, calls handler, pushes result.
                self.emit_expr(target);
                self.code.push(0x32); // TAWAIT
            }
            _ => {}
        }
    }

    /// Emit a TCALL to a named function.  Call this after `emit_program()` to
    /// create an entry point that executes a specific function (typically `main`).
    /// The TJMP in `emit_program` already points past all function bodies, so
    /// code appended here is what actually runs at startup.
    ///
    /// The function's return value will be on the stack when the VM halts.
    pub fn emit_entry_call(&mut self, func_name: &str) {
        if let Some(&addr) = self.func_addrs.get(func_name) {
            self.code.push(0x10); // TCALL — push return addr, jump to func
            self.code.extend_from_slice(&addr.to_le_bytes());
        }
    }

    pub fn finalize(mut self) -> Vec<u8> {
        self.code.push(0x00); // THALT
        self.code
    }

    fn patch_u16(&mut self, pos: usize, val: u16) {
        let bytes = val.to_le_bytes();
        self.code[pos] = bytes[0];
        self.code[pos + 1] = bytes[1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::vm::{BetVm, Value};

    #[test]
    fn test_compile_and_run_simple() {
        let input = "let x: trit = 1; let y: trit = -x; return y;";
        let mut parser = Parser::new(input);
        let mut emitter = BytecodeEmitter::new();
        
        // Parse and emit statements
        while let Ok(stmt) = parser.parse_stmt() {
            emitter.emit_stmt(&stmt);
        }
        
        let code = emitter.finalize();
        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        
        // Final 'y' should be in register 1
        assert_eq!(vm.get_register(1), Value::Trit(Trit::Reject));
    }

    #[test]
    fn test_sparseskip_emits_tsparse_matmul() {
        // @sparseskip on a let binding with matmul rhs should emit TSPARSE_MATMUL (0x21)
        // and the result tensor ref should be stored in a register
        let input = "let a: trittensor<2 x 2>; let b: trittensor<2 x 2>; @sparseskip let c: trittensor<2 x 2> = matmul(a, b);";
        let mut parser = Parser::new(input);
        let mut emitter = BytecodeEmitter::new();

        while let Ok(stmt) = parser.parse_stmt() {
            emitter.emit_stmt(&stmt);
        }

        let code = emitter.finalize();
        // Verify TSPARSE_MATMUL (0x21) appears in the bytecode
        assert!(code.contains(&0x21), "Expected TSPARSE_MATMUL (0x21) in bytecode");
        // Verify dense TMATMUL (0x20) does NOT appear (we used sparseskip)
        assert!(!code.contains(&0x20), "Expected no dense TMATMUL (0x20) when @sparseskip used");

        // Run it — both tensors are zero-initialized, result should be TensorRef in reg2
        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        assert!(matches!(vm.get_register(2), Value::TensorRef(_)));
    }

    #[test]
    fn test_compile_match() {
        let input = "let x: trit = 1; match x { 1 => { let y: trit = -1; } 0 => { let y: trit = 0; } -1 => { let y: trit = 1; } }";
        let mut parser = Parser::new(input);
        let mut emitter = BytecodeEmitter::new();
        
        while let Ok(stmt) = parser.parse_stmt() {
            emitter.emit_stmt(&stmt);
        }
        
        let code = emitter.finalize();
        let mut vm = BetVm::new(code);
        vm.run().unwrap();
        
        // 'x' is 1, so 'y' in the first branch should be -1.
        assert_eq!(vm.get_register(1), Value::Trit(Trit::Reject));
    }
}
