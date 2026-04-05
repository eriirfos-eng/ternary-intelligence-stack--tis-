//! Verilog module generator for BET primitives.
//!
//! Generates synthesisable Verilog-2001 that implements the BET ISA
//! using 2-bit wire-pair trit encoding.

/// A generated Verilog module: name + body text.
#[derive(Debug, Clone)]
pub struct VerilogModule {
    pub name: String,
    pub body: String,
}

impl VerilogModule {
    /// Write to a string with a standard header comment.
    pub fn render(&self) -> String {
        format!(
            "// ternlang-hdl: generated BET Verilog\n// Module: {}\n\n{}",
            self.name, self.body
        )
    }
}

/// Generates Verilog modules for BET primitives.
pub struct VerilogEmitter;

impl VerilogEmitter {
    /// trit_neg: inversion — maps +1↔-1, 0→0
    /// Encoding: swap t[1] and t[0] bits.
    pub fn trit_neg() -> VerilogModule {
        VerilogModule {
            name: "trit_neg".into(),
            body: r#"module trit_neg (
    input  [1:0] a,    // 2-bit trit: 01=-1, 10=+1, 11=0
    output [1:0] y
);
    // Negate: swap the two bits
    assign y = {a[0], a[1]};
endmodule"#.into(),
        }
    }

    /// trit_cons: consensus (ternary OR)
    /// consensus(+1, 0) = +1, consensus(-1, 0) = -1, consensus(+1, -1) = 0
    /// Truth table implemented as combinational logic.
    pub fn trit_cons() -> VerilogModule {
        VerilogModule {
            name: "trit_cons".into(),
            body: r#"module trit_cons (
    input  [1:0] a,
    input  [1:0] b,
    output [1:0] y
);
    // Consensus: if a == b, output a; else output hold (11)
    // Encoding: 01=-1, 10=+1, 11=0
    assign y = (a == b) ? a : 2'b11;
endmodule"#.into(),
        }
    }

    /// trit_mul: balanced ternary multiply
    /// Multiplication table: +1*+1=+1, +1*-1=-1, -1*-1=+1, 0*x=0
    pub fn trit_mul() -> VerilogModule {
        VerilogModule {
            name: "trit_mul".into(),
            body: r#"module trit_mul (
    input  [1:0] a,
    input  [1:0] b,
    output [1:0] y
);
    // If either operand is hold (11), output is hold (11).
    // If signs match, output is truth (10).
    // If signs differ, output is conflict (01).
    // Encoding: 01=-1, 10=+1, 11=0
    wire a_hold = (a == 2'b11);
    wire b_hold = (b == 2'b11);
    wire same   = (a == b);
    assign y = (a_hold || b_hold) ? 2'b11 :
               same               ? 2'b10 :
                                    2'b01;
endmodule"#.into(),
        }
    }

    /// trit_add: balanced ternary half-adder (sum + carry)
    /// Returns two trits: sum and carry.
    pub fn trit_add() -> VerilogModule {
        VerilogModule {
            name: "trit_add".into(),
            body: r#"module trit_add (
    input  [1:0] a,
    input  [1:0] b,
    output [1:0] sum,
    output [1:0] carry
);
    // Balanced ternary addition truth table (16 combinations):
    // Using case statement for clarity and synthesis friendliness.
    // Encoding: 01=-1, 10=+1, 11=0
    reg [3:0] result; // {carry[1:0], sum[1:0]}
    always @(*) begin
        case ({a, b})
            // BET: -1 + -1: sum = +1, carry = -1
            4'b0101: result = {2'b01, 2'b10};  // sum=+1, carry=-1
            // a=-1, b=0 → sum=-1, carry=0
            4'b0111: result = {2'b11, 2'b01};
            // a=-1, b=+1 → sum=0, carry=0
            4'b0110: result = {2'b11, 2'b11};
            // a=0, b=-1 → sum=-1, carry=0
            4'b1101: result = {2'b11, 2'b01};
            // a=0, b=0 → sum=0, carry=0
            4'b1111: result = {2'b11, 2'b11};
            // a=0, b=+1 → sum=+1, carry=0
            4'b1110: result = {2'b11, 2'b10};
            // a=+1, b=-1 → sum=0, carry=0
            4'b1001: result = {2'b11, 2'b11};
            // a=+1, b=0 → sum=+1, carry=0
            4'b1011: result = {2'b11, 2'b10};
            // a=+1, b=+1 → sum=-1, carry=+1
            4'b1010: result = {2'b10, 2'b01};
            default: result = {2'b11, 2'b11}; // fault → hold
        endcase
    end
    assign carry = result[3:2];
    assign sum   = result[1:0];

`ifdef FORMAL
    // Recommendation 4: Formal Verification for Ternary Carry Logic
    // Proves that valid ternary inputs never generate a FAULT (00) state.
    always @(*) begin
        if (a != 2'b00 && b != 2'b00) begin
            assert(sum != 2'b00);
            assert(carry != 2'b00);
        end
    end
`endif
endmodule"#.into(),
        }
    }

    /// trit_reg: synchronous ternary D-register, resets to hold (0).
    pub fn trit_reg() -> VerilogModule {
        VerilogModule {
            name: "trit_reg".into(),
            body: r#"module trit_reg (
    input        clk,
    input        rst_n,  // active low reset
    input  [1:0] d,
    output reg [1:0] q
);
    always @(posedge clk or negedge rst_n) begin
        if (!rst_n)
            q <= 2'b11;  // reset to hold (0)
        else
            q <= d;
    end
endmodule"#.into(),
        }
    }

    /// bet_alu: full BET ALU — supports ADD, MUL, NEG, CONS.
    /// op: 2'b00=ADD, 2'b01=MUL, 2'b10=NEG(a), 2'b11=CONS
    pub fn bet_alu() -> VerilogModule {
        VerilogModule {
            name: "bet_alu".into(),
            body: r#"module bet_alu (
    input  [1:0] a,
    input  [1:0] b,
    input  [1:0] op,     // 00=ADD, 01=MUL, 10=NEG, 11=CONS
    output [1:0] result,
    output [1:0] carry   // non-zero only for ADD
);
    wire [1:0] add_sum, add_carry;
    wire [1:0] mul_out;
    wire [1:0] neg_out;
    wire [1:0] cons_out;

    trit_add  u_add  (.a(a), .b(b), .sum(add_sum),  .carry(add_carry));
    trit_mul  u_mul  (.a(a), .b(b), .y(mul_out));
    trit_neg  u_neg  (.a(a),        .y(neg_out));
    trit_cons u_cons (.a(a), .b(b), .y(cons_out));

    assign result = (op == 2'b00) ? add_sum  :
                    (op == 2'b01) ? mul_out  :
                    (op == 2'b10) ? neg_out  :
                                    cons_out;
    assign carry  = (op == 2'b00) ? add_carry : 2'b11; // hold when not ADD
endmodule"#.into(),
        }
    }

    /// Parameterised N×N sparse matmul array.
    /// Generates a Verilog module with N² trit_mul cells,
    /// each with a zero-skip enable: zero-weighted cells clock-gate.
    pub fn sparse_matmul(n: usize) -> VerilogModule {
        let name = format!("sparse_matmul_{}x{}", n, n);
        let mut body = format!(
            r#"// N={n} sparse ternary matmul
// Recommendation 1: Pipelined design for improved clock frequency.
// Recommendation 3: Uses explicit clock enable (en) instead of clock gating.
module {name} #(parameter N = {n}) (
    input  clk,
    input  rst_n,
    input  en,                 // Clock enable
    input  [N*2-1:0] a_flat,   // input vector (N trits, 2 bits each)
    input  [N*N*2-1:0] w_flat, // weight matrix (N*N trits, 2 bits each)
    output [N*2-1:0] out_flat  // output vector (N trits)
);
    genvar row, col;
    // Pipelined partial products
    reg [1:0] prod_pipe [0:N-1][0:N-1];
    wire [1:0] prod [0:N-1][0:N-1];
    wire skip [0:N-1][0:N-1];

"#,
            n = n,
            name = name
        );

        body.push_str(r#"    // Generate multiply cells with zero-skip
    generate
        for (row = 0; row < N; row = row + 1) begin : gen_row
            for (col = 0; col < N; col = col + 1) begin : gen_col
                wire [1:0] w_ij = w_flat[(row*N+col)*2 +: 2];
                wire [1:0] a_j  = a_flat[col*2 +: 2];
                // Skip when weight is hold (2'b11)
                assign skip[row][col] = (w_ij == 2'b11);
                trit_mul u_mul (
                    .a(a_j),
                    .b(skip[row][col] ? 2'b11 : w_ij),
                    .y(prod[row][col])
                );
            end
        end
    endgenerate

    integer i, j;
    always @(posedge clk or negedge rst_n) begin
        if (!rst_n) begin
            for (i = 0; i < N; i = i + 1)
                for (j = 0; j < N; j = j + 1)
                    prod_pipe[i][j] <= 2'b11; // hold
        end else if (en) begin
            for (i = 0; i < N; i = i + 1)
                for (j = 0; j < N; j = j + 1)
                    prod_pipe[i][j] <= prod[i][j];
        end
    end

    // Recommendation 2: Standard Matmul Accumulation using trit_add.
    // For synthesis, we chain addition combinatorially, then register the final sum.
    wire [1:0] row_sum [0:N-1][0:N];
    wire [1:0] row_carry [0:N-1][0:N];
    
    generate
        for (row = 0; row < N; row = row + 1) begin : gen_acc
            assign row_sum[row][0] = 2'b11; // initial sum = hold
            assign row_carry[row][0] = 2'b11; // initial carry = hold
            for (col = 0; col < N; col = col + 1) begin : gen_add
                trit_add u_add (
                    .a(row_sum[row][col]),
                    .b(prod_pipe[row][col]),
                    .sum(row_sum[row][col+1]),
                    .carry(row_carry[row][col+1])
                );
            end
            
            // Output register (Pipeline stage 2)
            reg [1:0] final_acc;
            always @(posedge clk or negedge rst_n) begin
                if (!rst_n) final_acc <= 2'b11;
                else if (en) final_acc <= row_sum[row][N];
            end
            assign out_flat[row*2 +: 2] = final_acc;
        end
    endgenerate

endmodule"#);

        VerilogModule { name, body }
    }

    /// Emit all standard BET primitive modules as a single Verilog file.
    pub fn emit_primitives() -> String {
        let modules = vec![
            Self::trit_neg(),
            Self::trit_cons(),
            Self::trit_mul(),
            Self::trit_add(),
            Self::trit_reg(),
            Self::bet_alu(),
        ];
        let header = "// ternlang-hdl: BET Verilog Primitives\n// RFI-IRFOS Ternary Intelligence Stack\n// 2-bit wire-pair encoding: 01=-1, 10=+1, 11=0, 00=FAULT\n\n";
        let mut out = header.to_string();
        for m in modules {
            out.push_str(&m.body);
            out.push_str("\n\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trit_neg_renders() {
        let m = VerilogEmitter::trit_neg();
        let r = m.render();
        assert!(r.contains("module trit_neg"));
        assert!(r.contains("assign y = {a[0], a[1]}"));
    }

    #[test]
    fn test_trit_mul_renders() {
        let m = VerilogEmitter::trit_mul();
        let r = m.render();
        assert!(r.contains("module trit_mul"));
        assert!(r.contains("a_hold || b_hold"));
    }

    #[test]
    fn test_trit_add_renders() {
        let m = VerilogEmitter::trit_add();
        assert!(m.body.contains("module trit_add"));
        assert!(m.body.contains("carry"));
    }

    #[test]
    fn test_bet_alu_renders() {
        let m = VerilogEmitter::bet_alu();
        assert!(m.body.contains("module bet_alu"));
        assert!(m.body.contains("trit_add"));
        assert!(m.body.contains("trit_mul"));
    }

    #[test]
    fn test_sparse_matmul_generates() {
        let m = VerilogEmitter::sparse_matmul(4);
        assert!(m.name.contains("4x4"));
        assert!(m.body.contains("sparse_matmul_4x4"));
        assert!(m.body.contains("skip"));
    }

    #[test]
    fn test_emit_primitives_contains_all() {
        let out = VerilogEmitter::emit_primitives();
        assert!(out.contains("trit_neg"));
        assert!(out.contains("trit_cons"));
        assert!(out.contains("trit_mul"));
        assert!(out.contains("trit_add"));
        assert!(out.contains("trit_reg"));
        assert!(out.contains("bet_alu"));
    }
}
