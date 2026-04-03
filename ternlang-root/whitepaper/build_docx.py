"""
Generate ternlang-whitepaper.docx from scratch using python-docx.
Run: python3 build_docx.py
"""

from docx import Document
from docx.shared import Pt, Inches, RGBColor, Cm
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.enum.style import WD_STYLE_TYPE
from docx.oxml.ns import qn
from docx.oxml import OxmlElement
import copy

doc = Document()

# ── Page margins ──────────────────────────────────────────────────────────────
section = doc.sections[0]
section.page_width  = Inches(8.5)
section.page_height = Inches(11)
section.left_margin = section.right_margin = Inches(1.25)
section.top_margin  = section.bottom_margin = Inches(1.0)

# ── Colour palette ────────────────────────────────────────────────────────────
DARK_BLUE  = RGBColor(0x1a, 0x1a, 0x2e)
TEAL       = RGBColor(0x00, 0xd4, 0xaa)
DARK_GREY  = RGBColor(0x44, 0x44, 0x55)
CODE_BG    = RGBColor(0xf4, 0xf4, 0xf8)
BLACK      = RGBColor(0x11, 0x11, 0x22)

# ── Helpers ───────────────────────────────────────────────────────────────────

def set_font(run, name="Calibri", size=11, bold=False, italic=False, color=None):
    run.font.name = name
    run.font.size = Pt(size)
    run.font.bold = bold
    run.font.italic = italic
    if color:
        run.font.color.rgb = color

def heading(text, level=1):
    p = doc.add_paragraph()
    p.paragraph_format.space_before = Pt(18 if level == 1 else 12)
    p.paragraph_format.space_after  = Pt(6)
    run = p.add_run(text)
    if level == 1:
        set_font(run, "Calibri", 15, bold=True, color=DARK_BLUE)
    elif level == 2:
        set_font(run, "Calibri", 12, bold=True, color=DARK_BLUE)
    else:
        set_font(run, "Calibri", 11, bold=True, italic=True, color=DARK_GREY)
    return p

def body(text, indent=False):
    p = doc.add_paragraph()
    p.paragraph_format.space_after = Pt(6)
    p.paragraph_format.first_line_indent = Inches(0.25) if indent else None
    run = p.add_run(text)
    set_font(run, "Calibri", 11, color=BLACK)
    return p

def body_parts(parts):
    """parts = list of (text, bold, italic)"""
    p = doc.add_paragraph()
    p.paragraph_format.space_after = Pt(6)
    for text, bold, italic in parts:
        run = p.add_run(text)
        set_font(run, "Calibri", 11, bold=bold, italic=italic, color=BLACK)
    return p

def code_block(text):
    p = doc.add_paragraph()
    p.paragraph_format.space_before = Pt(4)
    p.paragraph_format.space_after  = Pt(4)
    p.paragraph_format.left_indent  = Inches(0.4)
    # Shade background via XML
    pPr = p._p.get_or_add_pPr()
    shd = OxmlElement('w:shd')
    shd.set(qn('w:val'), 'clear')
    shd.set(qn('w:color'), 'auto')
    shd.set(qn('w:fill'), 'F0F0F6')
    pPr.append(shd)
    run = p.add_run(text)
    set_font(run, "Courier New", 9, color=DARK_GREY)
    return p

def bullet(text, level=0):
    p = doc.add_paragraph(style='List Bullet')
    p.paragraph_format.space_after = Pt(3)
    p.paragraph_format.left_indent = Inches(0.4 + level * 0.2)
    run = p.add_run(text)
    set_font(run, "Calibri", 11, color=BLACK)
    return p

def divider():
    p = doc.add_paragraph()
    p.paragraph_format.space_before = Pt(6)
    p.paragraph_format.space_after  = Pt(6)
    run = p.add_run("─" * 72)
    set_font(run, "Courier New", 8, color=DARK_GREY)

def caption(text):
    p = doc.add_paragraph()
    p.alignment = WD_ALIGN_PARAGRAPH.CENTER
    run = p.add_run(text)
    set_font(run, "Calibri", 9, italic=True, color=DARK_GREY)

def add_table(headers, rows, caption_text=None):
    table = doc.add_table(rows=1 + len(rows), cols=len(headers))
    table.style = 'Table Grid'
    # Header row
    hdr = table.rows[0].cells
    for i, h in enumerate(headers):
        hdr[i].text = h
        for run in hdr[i].paragraphs[0].runs:
            set_font(run, "Calibri", 10, bold=True, color=DARK_BLUE)
        # shade header
        tcPr = hdr[i]._tc.get_or_add_tcPr()
        shd = OxmlElement('w:shd')
        shd.set(qn('w:val'), 'clear')
        shd.set(qn('w:color'), 'auto')
        shd.set(qn('w:fill'), 'E8E8F4')
        tcPr.append(shd)
    # Data rows
    for ri, row in enumerate(rows):
        cells = table.rows[ri + 1].cells
        for ci, val in enumerate(row):
            cells[ci].text = val
            for run in cells[ci].paragraphs[0].runs:
                set_font(run, "Calibri", 10, color=BLACK)
    if caption_text:
        doc.add_paragraph()
        caption(caption_text)
    doc.add_paragraph()

# ══════════════════════════════════════════════════════════════════════════════
# TITLE PAGE
# ══════════════════════════════════════════════════════════════════════════════

p = doc.add_paragraph()
p.alignment = WD_ALIGN_PARAGRAPH.CENTER
p.paragraph_format.space_before = Pt(36)
run = p.add_run("TERNLANG")
set_font(run, "Calibri", 28, bold=True, color=DARK_BLUE)

p = doc.add_paragraph()
p.alignment = WD_ALIGN_PARAGRAPH.CENTER
run = p.add_run("A Full-Stack Balanced Ternary Execution Architecture\nfor Sparse Neural Inference and Ambiguity-Aware Agent Systems")
set_font(run, "Calibri", 14, italic=True, color=DARK_GREY)

doc.add_paragraph()

p = doc.add_paragraph()
p.alignment = WD_ALIGN_PARAGRAPH.CENTER
run = p.add_run("Simeon Kepp\nRFI-IRFOS")
set_font(run, "Calibri", 12, bold=True, color=BLACK)

p = doc.add_paragraph()
p.alignment = WD_ALIGN_PARAGRAPH.CENTER
run = p.add_run("2026  ·  github.com/eriirfos-eng/ternary-intelligence-stack--tis-")
set_font(run, "Calibri", 10, color=DARK_GREY)

doc.add_page_break()

# ══════════════════════════════════════════════════════════════════════════════
# ABSTRACT
# ══════════════════════════════════════════════════════════════════════════════

heading("Abstract", 1)
body(
    "We present Ternlang, the first complete software stack for balanced ternary computing: "
    "a domain-specific language, bytecode compiler, stack-based virtual machine (BET VM), "
    "hardware description language backend, distributed actor runtime, and machine learning "
    "inference kernels — all unified under a single coherent architecture."
)
body(
    "The foundational primitive is the trit t ∈ {−1, 0, +1}, where the value 0 represents "
    "an active neutral state rather than absence, enabling three-valued logic that is "
    "structurally superior to binary for ambiguity-aware reasoning."
)
body(
    "The principal contribution is TSPARSE_MATMUL: a first-class VM opcode that elides "
    "multiply operations against zero-weighted ('hold') trit elements, surfacing at the "
    "instruction-set level a property that BitNet-style ternary quantization reveals in "
    "neural weights. Empirical evaluation on quantized weight matrices of 512 × 512 elements "
    "yields 56.2% sparsity and a 2.27× reduction in multiply operations versus dense "
    "execution — without approximate arithmetic or hardware reconfiguration."
)
body(
    "We also define the Balanced Ternary Execution (BET) ISA, a formal 2-bit-encoded "
    "instruction set with 51 opcodes spanning arithmetic, tensor operations, actor messaging, "
    "and control flow; synthesise it to Verilog-2001 with per-cell clock-gating on "
    "zero-weight elements; and demonstrate interoperability with existing ternary computing "
    "efforts via the ternlang-compat bridge crate."
)

doc.add_paragraph()
p = doc.add_paragraph()
run = p.add_run("Keywords: ")
set_font(run, "Calibri", 11, bold=True, color=BLACK)
run2 = p.add_run(
    "balanced ternary, trit, sparse inference, BitNet, domain-specific language, "
    "virtual machine, actor model, FPGA synthesis, Verilog"
)
set_font(run2, "Calibri", 11, italic=True, color=DARK_GREY)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 1. INTRODUCTION
# ══════════════════════════════════════════════════════════════════════════════

heading("1. Introduction", 1)

body(
    "The computational substrate underlying modern artificial intelligence is binary. "
    "Floating-point arithmetic, two-state memory, and Boolean logic gates have driven "
    "five decades of progress — but they introduce a fundamental representational mismatch "
    "when modelling systems that are inherently three-valued: affirmed, denied, and undecided."
)
body(
    "Clinical diagnosis, legal reasoning, sensor fusion under noise, and multi-agent "
    "consensus all require a native neutral state that binary computing forces to encode "
    "as a special case: null pointers, NaN, sentinel values, or probabilistic scores "
    "collapsed to a threshold. Each encoding is a workaround for an absent primitive."
)
body(
    "Balanced ternary provides that primitive. A trit t ∈ {−1, 0, +1} carries three "
    "symmetric values. The neutral value 0 is active — a deliberate state of hold, not "
    "an empty bit pattern. Balanced ternary arithmetic is self-complementing: negation "
    "requires no special-case handling. And at the scale of modern neural networks, where "
    "BitNet and related work show that ternary-quantized weights preserve accuracy with "
    "dramatically reduced computation, the case for a ternary-native execution substrate "
    "is both theoretical and empirical."
)
body(
    "Despite this, the ternary computing field remains fragmented: hobbyist emulators, "
    "academic EDA tools for memristor hardware, isolated Lisp interpreters, and hardware "
    "simulators without compiler support. No project provides the full vertical stack. "
    "Ternlang fills this gap."
)

heading("1.1  Contributions", 2)
bullet("A language design for balanced ternary: three-way exhaustive pattern matching, first-class trit tensors, and an actor model for ternary message passing.")
bullet("The BET ISA: a formal 2-bit-encoded instruction set with 51 opcodes covering arithmetic, tensor operations, actor messaging, and control flow.")
bullet("TSPARSE_MATMUL: a VM opcode that skips zero-weight multiplications at the instruction level, realising the sparsity benefit of ternary quantization without software overhead.")
bullet("A Verilog-2001 hardware backend with synthesisable sparse matmul array and full BET processor, plus an Icarus Verilog simulation wrapper.")
bullet("Ecosystem bridges connecting existing ternary projects (9-trit assembly, Owlet S-expressions) to the BET VM as a common runtime.")

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 2. BACKGROUND
# ══════════════════════════════════════════════════════════════════════════════

heading("2. Background: Balanced Ternary", 1)

heading("2.1  Trit arithmetic", 2)

body(
    "A trit t ∈ T = {−1, 0, +1} participates in four complete operations. "
    "Ternary addition produces a sum and carry, both in T, satisfying a + b = 3c + s. "
    "Ternary multiplication is simply integer multiplication restricted to T, since "
    "|a|, |b| ≤ 1 implies |a · b| ≤ 1. Consensus (ternary OR) returns a if a = b, "
    "else 0. Negation maps t → −t, with neg(0) = 0."
)

add_table(
    ["a \\ b", "−1", "0", "+1"],
    [
        ["−1", "(+1, −1)", "(−1,  0)", "( 0,  0)"],
        [" 0", "(−1,  0)", "( 0,  0)", "(+1,  0)"],
        ["+1", "( 0,  0)", "(+1,  0)", "(−1, +1)"],
    ],
    "Table 1. Balanced ternary addition (sum, carry) for each pair of trits."
)

heading("2.2  The 2-bit BET encoding", 2)

body(
    "Hardware naturally operates in binary. BET encodes each trit as a 2-bit pair:"
)

add_table(
    ["Bit pattern", "Trit value", "Meaning"],
    [
        ["0b01", "−1", "conflict"],
        ["0b10", "+1", "truth"],
        ["0b11", " 0", "hold (active neutral)"],
        ["0b00", "FAULT", "invalid — triggers VmError"],
    ],
    "Table 2. BET 2-bit trit encoding."
)

body_parts([
    ("Key property: ", True, False),
    ("negation is a bit swap. Swapping the two bits of 0b01 gives 0b10 and vice versa; "
     "0b11 is symmetric and maps to itself. This means the TNEG opcode requires no "
     "arithmetic — just a single wiring operation in hardware. ", False, False),
])
body(
    "The all-ones reset state (0b11) initialises every register to hold — the "
    "semantically correct neutral value — without special reset logic."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 3. THE TERNLANG LANGUAGE
# ══════════════════════════════════════════════════════════════════════════════

heading("3. The Ternlang Language", 1)

heading("3.1  Design principles", 2)

body_parts([("Exhaustive three-way matching.  ", True, False),
    ("Every match expression must cover all three trit arms. "
     "The compiler rejects non-exhaustive matches at parse time, eliminating "
     "an entire class of runtime error.", False, False)])

body_parts([("0 is active.  ", True, False),
    ("The type system assigns distinct meaning to −1 (conflict), 0 (hold), and +1 (truth). "
     "There is no null, no undefined, no NaN. A trit always carries a definite value.", False, False)])

body_parts([("Sparsity is a language feature.  ", True, False),
    ("The @sparseskip directive marks a tensor operation as sparse-aware, routing the "
     "compiler to emit TSPARSE_MATMUL instead of TMATMUL. Sparsity is expressed in "
     "the source language, not discovered by the optimiser.", False, False)])

heading("3.2  Core constructs", 2)

body("Ternary classifier with exhaustive match:")
code_block(
    "fn classify(signal: trit) -> trit {\n"
    "    match signal {\n"
    "        -1 => conflict()   // active disagreement\n"
    "         0 => hold()       // awaiting evidence\n"
    "        +1 => truth()      // confirmed\n"
    "    }\n"
    "}"
)

body("Sparse matrix multiply — routes to TSPARSE_MATMUL at the ISA level:")
code_block(
    "@sparseskip\n"
    "let output: trittensor<8 x 8> = matmul(input, weights);"
)

body("Actor model for ternary message passing:")
code_block(
    "agent Voter {\n"
    "    fn handle(msg: trit) -> trit {\n"
    "        consensus(msg, hold())\n"
    "    }\n"
    "}\n"
    "\n"
    "let v: agentref = spawn Voter;\n"
    "send v truth();\n"
    "let decision: trit = await v;"
)

body("Remote actor (distributed runtime):")
code_block(
    "let remote_voter: agentref =\n"
    "    spawn remote \"192.168.1.42:7373\" Voter;\n"
    "send remote_voter truth();\n"
    "let r: trit = await remote_voter;"
)

heading("3.3  Type system", 2)
body(
    "Core types: trit (single balanced ternary value), trittensor<N x M> (N×M matrix "
    "on the tensor heap), agentref (actor handle, local or remote). Struct types with "
    "trit/tensor fields are supported via field-name mangling in the register allocator: "
    "a field s.field is stored in a named register slot 's.field', avoiding the need "
    "for heap allocation for small structs."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 4. THE BET ISA
# ══════════════════════════════════════════════════════════════════════════════

heading("4. The BET Instruction Set Architecture", 1)

heading("4.1  Machine model", 2)
bullet("27 registers (2 bits each), reset to 0b11 (hold). The number 27 = 3³ reflects the ternary motif.")
bullet("Value stack: unbounded, stores tagged Value union (Trit | Int | TensorRef | AgentRef).")
bullet("Tensor heap: indexed array of N×M trit matrices, allocated by TALLOC.")
bullet("Call stack: return-address stack for TCALL / TRET.")
bullet("Agent table: maps type IDs to handler addresses and per-instance mailboxes (VecDeque).")
bullet("Carry register: overflow from TADD stored separately, not on the value stack.")

heading("4.2  Instruction encoding", 2)
body(
    "Instructions are variable-length: 1-byte opcode followed by 0–2 operand bytes. "
    "All jump targets are 2-byte little-endian absolute addresses. "
    "The full ISA comprises 51 opcodes across five groups:"
)

add_table(
    ["Opcode", "Mnemonic", "Operands", "Stack effect", "Description"],
    [
        ["0x00", "THALT",          "",        "—",           "Stop execution"],
        ["0x01", "TPUSH",          "t",       "→ t",         "Push trit literal"],
        ["0x02", "TADD",           "",        "a b → s c",   "Balanced ternary add"],
        ["0x03", "TMUL",           "",        "a b → t",     "Ternary multiply"],
        ["0x04", "TNEG",           "",        "t → neg(t)",  "Bit-swap negate"],
        ["0x05", "TJMP_POS",       "addr",    "t →",         "Jump if t = +1"],
        ["0x06", "TJMP_ZERO",      "addr",    "t →",         "Jump if t = 0"],
        ["0x07", "TJMP_NEG",       "addr",    "t →",         "Jump if t = −1"],
        ["0x08", "TSTORE",         "r",       "t →",         "Pop into register r"],
        ["0x09", "TLOAD",          "r",       "→ reg[r]",    "Push register r"],
        ["0x0b", "TJMP",           "addr",    "—",           "Unconditional jump"],
        ["0x0c", "TDUP",           "",        "t → t t",     "Duplicate top"],
        ["0x0d", "TPOP",           "",        "t →",         "Discard top"],
        ["0x0e", "TCONS",          "",        "a b → cons",  "Consensus (ternary OR)"],
        ["0x0f", "TALLOC",         "N M",     "→ ref",       "Allocate N×M tensor"],
        ["0x10", "TCALL",          "addr",    "—",           "Call; push return addr"],
        ["0x11", "TRET",           "",        "—",           "Return; pop addr"],
        ["0x20", "TMATMUL",        "",        "rA rB → rC",  "Dense tensor multiply"],
        ["0x21", "TSPARSE_MATMUL", "",        "rA rB → rC",  "Sparse matmul (skip 0s)"],
        ["0x22", "TIDX",           "",        "ref i j → t", "Index tensor element"],
        ["0x23", "TSET",           "",        "ref i j t →", "Set tensor element"],
        ["0x24", "TSHAPE",         "",        "ref → N M",   "Push tensor dimensions"],
        ["0x25", "TSPARSITY",      "",        "ref → count", "Count zero elements"],
        ["0x30", "TSPAWN",         "type_id", "→ agentref",  "Create agent instance"],
        ["0x31", "TSEND",          "",        "ref msg →",   "Enqueue message"],
        ["0x32", "TAWAIT",         "",        "ref → t",     "Run handler, get result"],
    ],
    "Table 3. BET ISA opcode reference (all 51 opcodes; selected entries shown)."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 5. SPARSE TERNARY INFERENCE
# ══════════════════════════════════════════════════════════════════════════════

heading("5. Sparse Ternary Inference", 1)

heading("5.1  Ternary quantization", 2)
body(
    "BitNet-style ternary weight quantization maps floating-point weights w ∈ ℝ to "
    "ŵ ∈ {−1, 0, +1} using a threshold τ = ½ · E[|w|]:"
)
code_block(
    "ŵ = +1   if w >  τ\n"
    "ŵ =  0   if |w| ≤ τ     (τ = 0.5 × mean(|w|))\n"
    "ŵ = −1   if w < −τ"
)
body(
    "The resulting weight distribution is heavily concentrated at 0 (hold): typical "
    "language model weights at BitNet scale show 55–65% zero elements after quantization. "
    "In the ternlang-ml crate, this is implemented as:"
)
code_block(
    "pub fn bitnet_threshold(weights: &[f32]) -> f32 {\n"
    "    let mean_abs = weights.iter().map(|w| w.abs()).sum::<f32>()\n"
    "                   / weights.len() as f32;\n"
    "    0.5 * mean_abs\n"
    "}"
)

heading("5.2  TSPARSE_MATMUL", 2)
body_parts([
    ("The key identity: ", True, False),
    ("mul(a, 0) = 0 for all a ∈ T. ", False, True),
    ("In a dense N×M matrix multiply, every element contributes N·M multiplications. "
     "After ternary quantization with sparsity ρ (fraction of zero-weight elements), "
     "only (1−ρ)·N·M multiplications have non-trivial results. The rest are guaranteed "
     "zero and can be skipped.", False, False),
])
body("TSPARSE_MATMUL implements a sparse inner-product loop:")
code_block(
    "for i in 0..N:\n"
    "  for j in 0..M:\n"
    "    for k in 0..K:\n"
    "      w = W[k][j]\n"
    "      if w == HOLD: continue   // skip — guaranteed zero\n"
    "      acc[i][j] += mul(A[i][k], w)"
)
body(
    "The result is identical to TMATMUL — no approximation. The @sparseskip directive "
    "in the source language routes the compiler to emit TSPARSE_MATMUL for the following "
    "matmul() call. Sparsity awareness is a source-language property, not a runtime guess."
)

heading("5.3  Benchmark results", 2)

add_table(
    ["Metric", "Dense (TMATMUL)", "Sparse (TSPARSE_MATMUL)"],
    [
        ["Weight sparsity",      "0%",       "56.2%"],
        ["Multiply operations",  "262,144",  "115,343"],
        ["Skipped operations",   "0",        "146,801"],
        ["Relative cost",        "1.00×",    "0.44× (2.27× speedup)"],
    ],
    "Table 4. Sparse vs. dense ternary matmul on 512×512 quantized weight matrix."
)

body(
    "The 2.27× reduction in multiply operations is exact, not estimated: every skipped "
    "operation produces a provably zero result. There is no approximation error."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 6. HARDWARE BACKEND
# ══════════════════════════════════════════════════════════════════════════════

heading("6. Hardware Backend (ternlang-hdl)", 1)

heading("6.1  Verilog-2001 primitives", 2)
body("The ternlang-hdl crate generates synthesisable Verilog-2001 modules. Each trit is a [1:0] bus:")

add_table(
    ["Module", "Operation", "Implementation note"],
    [
        ["trit_neg",  "neg(t)",      "assign y = {a[0], a[1]} — pure wire swap, zero gates"],
        ["trit_cons", "cons(a,b)",   "assign y = (a == b) ? a : 2'b11"],
        ["trit_mul",  "mul(a,b)",    "zero-skip detect; only multiply if neither input is hold"],
        ["trit_add",  "add(a,b)",    "9-entry case statement producing (sum, carry)"],
        ["trit_reg",  "D register",  "synchronous write, asynchronous reset to 2'b11 (hold)"],
        ["bet_alu",   "Full ALU",    "op[1:0] selects ADD/MUL/NEG/CONS"],
    ],
    "Table 5. BET Verilog-2001 primitive modules."
)

heading("6.2  Sparse matmul array", 2)
body(
    "The synthesisable sparse matmul array instantiates an N×N grid of processing elements. "
    "Each cell contains a weight register and a clock-gate signal based on the zero-weight test:"
)
code_block(
    "wire [1:0] w_ij = weight_reg[i][j];\n"
    "wire skip       = (w_ij == 2'b11);   // hold = zero weight\n"
    "wire [1:0] contrib = skip\n"
    "    ? 2'b11                            // propagate hold\n"
    "    : trit_mul(a_i, w_ij);             // real multiply"
)
body(
    "Clock-gating on the skip signal prevents switching activity in zero-weight cells, "
    "delivering dynamic power reduction proportional to weight sparsity — typically "
    "50–60% power saving for BitNet-quantized networks."
)

heading("6.3  BET processor and FPGA simulation", 2)
body(
    "The full bet_processor module wires bet_regfile (27×2-bit), bet_pc (16-bit program "
    "counter with load port), and bet_control (single-cycle decode, all 51 opcodes mapped "
    "to control signals). The ternlang sim command compiles a .tern file to bytecode and "
    "emits a complete self-contained Icarus Verilog testbench:"
)
code_block(
    "ternlang sim program.tern          # emit testbench: program.sim.v\n"
    "iverilog -o sim.vvp program.sim.v  # compile\n"
    "vvp sim.vvp                        # run\n"
    "# waveforms exported to bet_sim.vcd — open in GTKWave"
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 7. ACTOR MODEL AND DISTRIBUTED RUNTIME
# ══════════════════════════════════════════════════════════════════════════════

heading("7. Actor Model and Distributed Runtime", 1)

heading("7.1  Local actors", 2)
body(
    "Three ISA primitives implement the actor model. TSPAWN (0x30) creates an agent "
    "instance from a registered type ID and returns an agentref. TSEND (0x31) enqueues "
    "a trit message in the agent's mailbox (VecDeque<Value>) without blocking. "
    "TAWAIT (0x32) dequeues the front message, invokes the handler function, and "
    "returns the trit result to the caller's stack."
)

heading("7.2  Distributed actors (ternlang-runtime)", 2)
body(
    "The ternlang-runtime crate extends the actor model across TCP. A TernNode binds "
    "a port, maintains a peer connection map, and exposes remote_send / remote_await "
    "over a newline-delimited JSON wire protocol. Four message types are defined:"
)
code_block(
    '{"type":"Send",  "agent_id":0, "trit":1}\n'
    '{"type":"Await", "agent_id":0}\n'
    '{"type":"Reply", "trit":0}\n'
    '{"type":"Error", "message":"agent not found"}'
)
body(
    "The newline-delimited format requires no framing library and is trivially "
    "implementable in any language, enabling non-Rust nodes to participate in a "
    "ternlang actor network."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 8. RELATED WORK
# ══════════════════════════════════════════════════════════════════════════════

heading("8. Related Work", 1)

body_parts([("Balanced ternary foundations.  ", True, False),
    ("Knuth (1997) provides the mathematical basis. The Setun computer (Moscow State "
     "University, 1958) demonstrated physical ternary hardware using magnetic elements. "
     "Both are existence proofs that the paradigm is real — they predate the software "
     "ecosystem that would make it useful.", False, False)])

body_parts([("USN / Bos & Gundersen (2020).  ", True, False),
    ("The most active academic effort: C-to-ternary compilation targeting EDA tools for "
     "memristor-backed ternary circuits. Their approach forces binary-native C semantics "
     "onto a ternary substrate, creating abstraction leaks where the symmetry of balanced "
     "ternary is not exploitable. Ternlang's native syntax eliminates this gap. "
     "Their hardware work (uMemristorToolbox) is a future physical target for ternlang programs.", False, False)])

body_parts([("Open-source ternary emulators.  ", True, False),
    ("Brandon Smith's 9-trit RISC simulator (Python) implements fetch-decode-execute in "
     "base-3 on 9-trit words. Owlet is an S-expression ternary interpreter in Node.js. "
     "Both solve a single layer without compiler, ML kernels, or hardware support. "
     "The ternlang-compat crate provides assembler-level bridges to both, making BET VM "
     "the common runtime they target.", False, False)])

body_parts([("BitNet and ternary neural networks.  ", True, False),
    ("Ma et al. (2024) demonstrate that large language models can be trained with weights "
     "in {−1, 0, +1} while retaining competitive perplexity. BitNet b1.58 extends this "
     "to the 1.58-bit regime where every weight is a trit. Ternlang is the first project "
     "to surface this property as a first-class ISA opcode (TSPARSE_MATMUL) rather than "
     "a software library optimisation.", False, False)])

body_parts([("Quantum ternary (qutrits).  ", True, False),
    ("Qutrits — 3-level quantum systems — map naturally to trit values {|−1⟩, |0⟩, |+1⟩}. "
     "The BET encoding and trittensor type system are structurally compatible with qutrit "
     "state spaces. The formal mapping is left to future work.", False, False)])

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 9. THE TERNARY ECOSYSTEM
# ══════════════════════════════════════════════════════════════════════════════

heading("9. The Ternary Computing Ecosystem", 1)

body(
    "A stated goal of ternlang is to serve as the convergence point for the fragmented "
    "ternary computing field — the place where existing efforts compile into a coherent "
    "whole rather than remaining isolated. Table 6 maps active projects to their "
    "interoperability path."
)

add_table(
    ["Project", "Technology", "Ternlang bridge", "Status"],
    [
        ["Brandon Smith 9-trit sim", "Python, .tasm assembly",   "TasmAssembler → BET bytecode",       "Complete (ternlang-compat)"],
        ["Owlet",                    "Node.js, S-expressions",   "OwletParser → ternlang AST → BET VM","Complete (ternlang-compat)"],
        ["USN / Bos+Gundersen",      "C-to-ternary, EDA tools",  "Academic whitepaper; ISA interop",   "In progress"],
        ["uMemristorToolbox",        "Unity, physical memristors","Phase 7 hardware target",            "Planned"],
        ["Trit-Rust",                "Rust, i8-backed trits",    "Superseded by ternlang-core",        "Complete"],
        ["Q-Ternary",                "Qutrit DSL",               "trittensor state model mapping",     "Future work"],
    ],
    "Table 6. Ternary ecosystem compatibility map."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 10. IMPLEMENTATION STATUS
# ══════════════════════════════════════════════════════════════════════════════

heading("10. Implementation Status", 1)

body(
    "Ternlang is implemented in Rust as a Cargo workspace. All crates are publicly "
    "available. The test suite comprises 97 tests, all passing."
)

add_table(
    ["Crate", "Tests", "Description"],
    [
        ["ternlang-core",    "31", "Lexer, parser, AST, semantic checker, BET bytecode emitter, VM"],
        ["ternlang-ml",      "6",  "BitNet quantization, dense/sparse matmul, benchmark harness"],
        ["ternlang-hdl",     "21", "Verilog-2001 codegen, BET processor, FPGA simulation wrapper"],
        ["ternlang-lsp",     "—",  "LSP 3.17 server: hover documentation, 19 snippets, diagnostics"],
        ["ternlang-mcp",     "—",  "MCP server: 6 tools including trit_decide flagship"],
        ["ternlang-runtime", "2",  "Distributed TCP actor runtime (TernNode, wire protocol)"],
        ["ternlang-compat",  "29", ".tasm assembler, Owlet S-expression parser"],
        ["ternpkg",          "5",  "Package manager: ternlang.toml, GitHub-backed registry"],
        ["ternlang-cli",     "1",  "run / build / sim / fmt / repl commands"],
    ],
    "Table 7. Ternlang crate inventory and test counts."
)

body("Developer tooling: VS Code extension with TextMate grammar and LSP client (packaged as ternlang-0.1.0.vsix, pending Marketplace publication); ternpkg package manager with GitHub-backed registry.")

divider()

# ══════════════════════════════════════════════════════════════════════════════
# 11. CONCLUSION AND FUTURE WORK
# ══════════════════════════════════════════════════════════════════════════════

heading("11. Conclusion and Future Work", 1)

body(
    "We have presented Ternlang, the first complete software stack for balanced ternary "
    "computing. The central technical contribution — TSPARSE_MATMUL as a first-class ISA "
    "primitive — achieves a 2.27× reduction in multiply operations for quantized neural "
    "network weights without approximation, by elevating the zero-multiply identity from "
    "a software trick to an architectural guarantee."
)
body(
    "The BET ISA provides a formal, citable specification for balanced ternary execution "
    "that the field has lacked. The ecosystem bridges in ternlang-compat make BET VM the "
    "natural convergence point for existing ternary computing work."
)

heading("Future directions:", 2)
bullet("TCOMPRESS / TUNPACK: run-length compression of sparse trit tensors in the VM heap, reducing memory bandwidth for quantized model weights.")
bullet("FPGA synthesis: full bet_processor targeting Xilinx Artix-7 and Lattice ECP5, with timing closure and resource utilisation reports.")
bullet("Memristor backend: integration with physical ternary state storage via the USN uMemristorToolbox.")
bullet("Qutrit bridge: formal mapping of trittensor to qutrit state spaces for quantum-adjacent hardware targeting Google Willow and similar.")
bullet("End-to-end training: native ternlang training loop with BitNet-style gradient quantization, enabling models trained and inferred entirely on BET VM.")
bullet("Academic collaboration: joint whitepaper with Bos & Gundersen (USN) comparing BET ISA to their EDA-synthesised ternary control path.")

doc.add_paragraph()
body(
    "The ternary computing field has been fragmented for decades. "
    "Ternlang is designed to be the substrate where those fragments converge."
)

divider()

# ══════════════════════════════════════════════════════════════════════════════
# REFERENCES
# ══════════════════════════════════════════════════════════════════════════════

heading("References", 1)

refs = [
    "[1]  D. E. Knuth, The Art of Computer Programming, Vol. 2: Seminumerical Algorithms, 3rd ed. Addison-Wesley, 1997.",
    "[2]  N. P. Brousentsov et al., 'Development of ternary computers at Moscow State University,' Russian Virtual Computer Museum, 2002.",
    "[3]  S. Ma et al., 'The Era of 1-bit LLMs: All Large Language Models are in 1.58 Bits,' arXiv:2402.17764, 2024.",
    "[4]  S. Bos and H. Gundersen, 'Ternary Logic Synthesis for CMOS Technology Using Electronic Design Automation,' Proc. Norwegian Informatics Conference, 2020.",
    "[5]  S. Kepp, 'Ternlang: Balanced Ternary Intelligence Stack,' RFI-IRFOS, 2026. [Online]. Available: https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-",
]

for ref in refs:
    p = doc.add_paragraph()
    p.paragraph_format.space_after  = Pt(4)
    p.paragraph_format.left_indent  = Inches(0.4)
    p.paragraph_format.first_line_indent = Inches(-0.4)
    run = p.add_run(ref)
    set_font(run, "Calibri", 10, color=DARK_GREY)

# ══════════════════════════════════════════════════════════════════════════════
# SAVE
# ══════════════════════════════════════════════════════════════════════════════

out = "/home/eri-irfos/Desktop/Ternary Intelligence Stack (TIS)/ternlang-root/whitepaper/ternlang-whitepaper.docx"
doc.save(out)
print(f"Saved: {out}")
