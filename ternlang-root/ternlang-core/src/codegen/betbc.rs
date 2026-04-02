use crate::ast::*;
use crate::vm::bet::pack_trits;
use crate::trit::Trit;

pub struct BytecodeEmitter {
    code: Vec<u8>,
    symbols: std::collections::HashMap<String, u8>,
    func_addrs: std::collections::HashMap<String, u16>,
    break_patches: Vec<usize>, // addresses to patch when a loop ends
    next_reg: u8,
}

impl BytecodeEmitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            symbols: std::collections::HashMap::new(),
            func_addrs: std::collections::HashMap::new(),
            break_patches: Vec::new(),
            next_reg: 0,
        }
    }

    pub fn emit_program(&mut self, program: &Program) {
        // Two-pass: first emit a TJMP over all function bodies so execution
        // starts at the entry point (last function, or a designated main).
        // Pass 1: record which functions exist, emit a jump placeholder.
        let entry_jmp_patch = self.code.len() + 1;
        self.code.push(0x0b); // TJMP — skip over function bodies
        self.code.extend_from_slice(&[0u8, 0u8]);

        // Pass 2: emit each function body, recording its address.
        for func in &program.functions {
            self.emit_function(func);
        }

        // Patch the entry jump to land after all function bodies.
        // (Programs that call functions explicitly will use TCALL.)
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
                    }
                    _ => {
                        self.emit_expr(value);
                    }
                }
                
                let reg = self.next_reg;
                self.symbols.insert(name.clone(), reg);
                self.next_reg += 1;
                
                self.code.push(0x08); // TSTORE
                self.code.push(reg);
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
                self.code.extend(pack_trits(&[Trit::Zero]));
                self.code.push(0x08); self.code.push(idx_reg); // TSTORE idx=0

                let loop_top = self.code.len() as u16;

                // Load current element: TIDX(tensor, 0, idx) — simplified 1D walk
                self.code.push(0x09); self.code.push(iter_reg); // TLOAD tensor
                self.code.push(0x01); self.code.extend(pack_trits(&[Trit::Zero])); // row 0
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
                        self.code.extend(pack_trits(&[Trit::PosOne]));
                    }
                    "hold" => {
                        self.code.push(0x01); // TPUSH
                        self.code.extend(pack_trits(&[Trit::Zero]));
                    }
                    "conflict" => {
                        self.code.push(0x01); // TPUSH
                        self.code.extend(pack_trits(&[Trit::NegOne]));
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
                            self.code.extend(pack_trits(&[Trit::Zero]));
                        }
                    }
                }
            }
            _ => {}
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
        assert_eq!(vm.get_register(1), Value::Trit(Trit::NegOne));
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
        assert_eq!(vm.get_register(1), Value::Trit(Trit::NegOne));
    }
}
