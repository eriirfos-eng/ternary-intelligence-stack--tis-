use crate::ast::*;
use crate::vm::bet::pack_trits;
use crate::trit::Trit;

pub struct BytecodeEmitter {
    code: Vec<u8>,
    symbols: std::collections::HashMap<String, u8>,
    func_addrs: std::collections::HashMap<String, u16>, // function name → bytecode address
    next_reg: u8,
}

impl BytecodeEmitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            symbols: std::collections::HashMap::new(),
            func_addrs: std::collections::HashMap::new(),
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
            Stmt::Return(expr) => {
                self.emit_expr(expr);
                self.code.push(0x00); // THALT
            }
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
            }
            Stmt::Decorated { stmt, .. } => {
                // TODO: Apply directive to following statement
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
                    BinOp::Add => self.code.push(0x02),
                    BinOp::Mul => self.code.push(0x03),
                    _ => {}
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
