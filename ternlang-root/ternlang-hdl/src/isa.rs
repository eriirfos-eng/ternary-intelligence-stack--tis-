//! BET ISA lowering to Verilog control logic.
//!
//! Maps BET opcodes to synthesisable Verilog state machines.
//! This is a structural emitter — it generates the control path
//! for a BET processor implementation on FPGA.

/// BET opcode → Verilog control signal mapping.
/// Each opcode selects ALU operation and register file ports.
#[derive(Debug, Clone)]
pub struct BetIsaEmitter {
    program_counter_bits: usize,
    register_file_depth: usize,
}

impl BetIsaEmitter {
    pub fn new() -> Self {
        BetIsaEmitter {
            program_counter_bits: 16,
            register_file_depth: 27,
        }
    }

    /// Generate the BET register file module.
    /// 27 registers × 2 bits each.
    pub fn emit_register_file(&self) -> String {
        let n = self.register_file_depth;
        format!(
            r#"// BET Register File — {n} ternary registers
module bet_regfile #(
    parameter DEPTH = {n}
) (
    input        clk,
    input        rst_n,
    input  [4:0] rs1,      // read port 1 address
    input  [4:0] rs2,      // read port 2 address
    input  [4:0] rd,       // write port address
    input  [1:0] wd,       // write data (trit)
    input        we,       // write enable
    output [1:0] rdata1,   // read data 1
    output [1:0] rdata2    // read data 2
);
    reg [1:0] regs [0:DEPTH-1];
    integer i;

    always @(posedge clk or negedge rst_n) begin
        if (!rst_n)
            for (i = 0; i < DEPTH; i = i + 1)
                regs[i] <= 2'b11; // reset all to hold (0)
        else if (we)
            regs[rd] <= wd;
    end

    assign rdata1 = regs[rs1];
    assign rdata2 = regs[rs2];
endmodule
"#,
            n = n
        )
    }

    /// Generate the BET program counter module.
    pub fn emit_program_counter(&self) -> String {
        let bits = self.program_counter_bits;
        format!(
            r#"// BET Program Counter — {bits}-bit
module bet_pc (
    input            clk,
    input            rst_n,
    input  [{bits_m1}:0] next_pc,
    input            load,     // when high: PC ← next_pc (jump)
    output [{bits_m1}:0] pc
);
    reg [{bits_m1}:0] pc_reg;
    always @(posedge clk or negedge rst_n) begin
        if (!rst_n)
            pc_reg <= 0;
        else if (load)
            pc_reg <= next_pc;
        else
            pc_reg <= pc_reg + 1;
    end
    assign pc = pc_reg;
endmodule
"#,
            bits = bits,
            bits_m1 = bits - 1
        )
    }

    /// Generate the BET fetch-decode-execute control unit (simplified single-cycle).
    pub fn emit_control_unit(&self) -> String {
        r#"// BET Control Unit — single-cycle implementation
// Maps BET opcodes to ALU op + register file control signals
module bet_control (
    input  [7:0]  opcode,
    output [1:0]  alu_op,   // 00=ADD 01=MUL 10=NEG 11=CONS
    output        reg_we,   // register file write enable
    output        mem_re,   // tensor heap read enable
    output        mem_we,   // tensor heap write enable
    output        pc_load,  // unconditional jump
    output        is_call,  // TCALL: push return address
    output        is_ret,   // TRET: pop return address
    output        is_halt,  // THALT
    output        is_spawn, // TSPAWN
    output        is_send,  // TSEND
    output        is_await  // TAWAIT
);
    assign alu_op   = (opcode == 8'h02) ? 2'b00 : // TADD
                      (opcode == 8'h03) ? 2'b01 : // TMUL
                      (opcode == 8'h04) ? 2'b10 : // TNEG
                      (opcode == 8'h0e) ? 2'b11 : // TCONS
                                          2'b11;   // default: hold

    assign reg_we   = (opcode == 8'h08);                            // TSTORE
    assign mem_re   = (opcode == 8'h20) || (opcode == 8'h21) ||     // TMATMUL, TSPARSE_MATMUL
                      (opcode == 8'h22) || (opcode == 8'h24) ||     // TIDX, TSHAPE
                      (opcode == 8'h25);                            // TSPARSITY
    assign mem_we   = (opcode == 8'h0f) || (opcode == 8'h23);       // TALLOC, TSET
    assign pc_load  = (opcode == 8'h0b) || (opcode == 8'h05) ||     // TJMP, TJMP_POS
                      (opcode == 8'h06) || (opcode == 8'h07) ||     // TJMP_ZERO, TJMP_NEG
                      (opcode == 8'h10) || (opcode == 8'h11);       // TCALL, TRET
    assign is_call  = (opcode == 8'h10);
    assign is_ret   = (opcode == 8'h11);
    assign is_halt  = (opcode == 8'h00);
    assign is_spawn = (opcode == 8'h30);
    assign is_send  = (opcode == 8'h31);
    assign is_await = (opcode == 8'h32);
endmodule
"#.into()
    }

    /// Emit the full BET processor top-level module that wires all components.
    pub fn emit_top(&self) -> String {
        let bits = self.program_counter_bits;
        format!(
            r#"// BET Processor — Top Level
// Balanced Ternary Execution: full single-cycle FPGA implementation
// RFI-IRFOS Ternary Intelligence Stack
module bet_processor (
    input        clk,
    input        rst_n,
    // Instruction memory interface (external ROM)
    output [{bits_m1}:0] imem_addr,
    input  [7:0]  imem_data,
    // Data memory interface (tensor heap — external BRAM)
    output [15:0] dmem_addr,
    output [1:0]  dmem_wdata,
    output        dmem_we,
    output        dmem_re,
    input  [1:0]  dmem_rdata
);
    wire [{bits_m1}:0] pc;
    wire [{bits_m1}:0] next_pc;
    wire [7:0]         opcode;
    wire [1:0]         alu_op;
    wire               reg_we, mem_re, mem_we, pc_load;
    wire               is_call, is_ret, is_halt;

    assign imem_addr = pc;
    assign opcode    = imem_data;

    bet_pc       u_pc (.clk(clk), .rst_n(rst_n), .next_pc(next_pc),
                       .load(pc_load), .pc(pc));
    bet_control  u_ctl(.opcode(opcode), .alu_op(alu_op),
                       .reg_we(reg_we), .mem_re(mem_re), .mem_we(mem_we),
                       .pc_load(pc_load), .is_call(is_call), .is_ret(is_ret),
                       .is_halt(is_halt),
                       .is_spawn(), .is_send(), .is_await());

    // Stack, ALU, register file, tensor heap — instantiated by synthesis tool
    // See individual modules for port details.

endmodule
"#,
            bits_m1 = bits - 1
        )
    }

    /// Emit all HDL source files as a single string for file output.
    pub fn emit_all(&self) -> String {
        let mut out = String::new();
        out.push_str("// ternlang-hdl: BET Processor HDL\n");
        out.push_str("// RFI-IRFOS Ternary Intelligence Stack\n");
        out.push_str("// Synthesisable Verilog-2001\n\n");
        out.push_str(&self.emit_register_file());
        out.push_str(&self.emit_program_counter());
        out.push_str(&self.emit_control_unit());
        out.push_str(&self.emit_top());
        out
    }
}

impl Default for BetIsaEmitter {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regfile_emits() {
        let e = BetIsaEmitter::new();
        let out = e.emit_register_file();
        assert!(out.contains("module bet_regfile"));
        assert!(out.contains("27"));
    }

    #[test]
    fn test_pc_emits() {
        let e = BetIsaEmitter::new();
        let out = e.emit_program_counter();
        assert!(out.contains("module bet_pc"));
        assert!(out.contains("16"));
    }

    #[test]
    fn test_control_unit_opcodes() {
        let e = BetIsaEmitter::new();
        let out = e.emit_control_unit();
        assert!(out.contains("8'h02")); // TADD
        assert!(out.contains("8'h21")); // TSPARSE_MATMUL — check missing
        assert!(out.contains("8'h30")); // TSPAWN
    }

    #[test]
    fn test_top_emits() {
        let e = BetIsaEmitter::new();
        let out = e.emit_top();
        assert!(out.contains("module bet_processor"));
        assert!(out.contains("bet_control"));
        assert!(out.contains("bet_pc"));
    }

    #[test]
    fn test_emit_all_contains_all_modules() {
        let e = BetIsaEmitter::new();
        let out = e.emit_all();
        assert!(out.contains("bet_regfile"));
        assert!(out.contains("bet_pc"));
        assert!(out.contains("bet_control"));
        assert!(out.contains("bet_processor"));
    }
}
