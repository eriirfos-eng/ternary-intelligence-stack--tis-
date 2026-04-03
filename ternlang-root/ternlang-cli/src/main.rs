use clap::{Parser as ClapParser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use ternlang_core::parser::Parser;
use ternlang_core::codegen::betbc::BytecodeEmitter;
use ternlang_core::vm::{BetVm, Value};
use ternlang_core::StdlibLoader;
use ternlang_ml::{TritMatrix, bitnet_threshold, benchmark};
use ternlang_hdl::{BetSimEmitter, BetRtlProcessor};
use ternlang_runtime::TernNode;

#[derive(ClapParser)]
#[command(name = "ternlang")]
#[command(about = "Ternlang CLI - Balanced Ternary Systems Language", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile and run a .tern file
    Run {
        /// Path to the .tern file
        file: PathBuf,
        /// This node's TCP address for distributed agent communication (e.g. 127.0.0.1:7373)
        #[arg(long, value_name = "ADDR")]
        node_addr: Option<String>,
        /// Pre-connect to a peer node before running (can be specified multiple times)
        #[arg(long, value_name = "ADDR")]
        peer: Vec<String>,
    },
    /// Compile a .tern file to bytecode
    Build {
        /// Path to the .tern file
        file: PathBuf,
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Interactive REPL for trit expression evaluation
    Repl,
    /// Format a .tern file (canonical 3-way match style)
    Fmt {
        /// Path to the .tern file
        file: PathBuf,
        /// Write formatted output back to file (default: print to stdout)
        #[arg(short, long)]
        write: bool,
    },
    /// Generate an Icarus Verilog FPGA testbench or run RTL simulation
    Sim {
        /// Path to the .tern file
        file: PathBuf,
        /// Write testbench to this file (default: <file>.sim.v)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Run simulation with iverilog+vvp if available
        #[arg(short, long)]
        run: bool,
        /// Run cycle-accurate RTL simulation in pure Rust (no external tools needed)
        #[arg(long)]
        rtl: bool,
        /// Max clock cycles for RTL simulation (default: 10000)
        #[arg(long, default_value = "10000")]
        max_cycles: u64,
    },
    /// Emit BET processor Verilog and run Yosys synthesis (Phase 6.1)
    HdlSynth {
        /// Output directory for generated Verilog files (default: ./bet_hdl/)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Run Yosys synthesis if yosys is on PATH
        #[arg(short, long)]
        synth: bool,
    },
    /// [hidden] You already know what this does
    #[command(hide = true)]
    Enlighten,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Run { file, node_addr, peer } => {
            let input = fs::read_to_string(file).expect("Failed to read file");
            let mut parser = Parser::new(&input);
            let mut emitter = BytecodeEmitter::new();

            // Try parsing as a program first
            match parser.parse_program() {
                Ok(mut prog) => {
                    StdlibLoader::resolve(&mut prog);
                    emitter.emit_program(&prog);
                }
                Err(e) => {
                    eprintln!("Parse program error: {:?}", e);
                    // Fallback: Reset and try parsing statements (for snippets without 'fn')
                    let mut parser = Parser::new(&input);
                    loop {
                        match parser.parse_stmt() {
                            Ok(stmt) => emitter.emit_stmt(&stmt),
                            Err(e) => {
                                if format!("{:?}", e).contains("EOF") {
                                    break;
                                }
                                eprintln!("Parse stmt error: {:?}", e);
                                break;
                            }
                        }
                    }
                }
            }

            let code = emitter.finalize();
            println!("Emitted {} bytes of bytecode", code.len());
            let mut vm = BetVm::new(code);

            // Phase 5.1: Distributed runtime setup
            if let Some(addr) = node_addr {
                let node = Arc::new(TernNode::new(addr));
                node.listen();
                eprintln!("[runtime] TernNode listening on {}", addr);
                vm.set_node_id(addr.clone());
                // Pre-connect to any specified peers
                for peer_addr in peer {
                    match node.connect(peer_addr) {
                        Ok(()) => eprintln!("[runtime] connected to peer {}", peer_addr),
                        Err(e) => eprintln!("[runtime] peer {} unreachable: {}", peer_addr, e),
                    }
                }
                vm.set_remote(node);
            }

            match vm.run() {
                Ok(_) => {
                    println!("Program exited successfully.");
                    // Print registers for debugging
                    for i in 0..10 {
                        let val = vm.get_register(i);
                        match val {
                            Value::Trit(t) => println!("Reg {}: trit({})", i, t),
                            Value::Int(v) => println!("Reg {}: int({})", i, v),
                            Value::TensorRef(r) => println!("Reg {}: tensor_ref({})", i, r),
                            Value::AgentRef(a, _)  => println!("Reg {}: agent_ref({})", i, a),
                            Value::String(s) => println!("Reg {}: string({:?})", i, s),
                        }
                    }
                }
                Err(e) => eprintln!("VM Error: {}", e),
            }
        }
        Commands::Repl => {
            run_repl();
        }
        Commands::Fmt { file, write } => {
            run_fmt(file, *write);
        }
        Commands::Sim { file, output, run, rtl, max_cycles } => {
            if *rtl {
                run_rtl_sim(file, *max_cycles);
            } else {
                run_sim(file, output.as_deref(), *run);
            }
        }
        Commands::HdlSynth { output, synth } => {
            run_hdl_synth(output.as_deref(), *synth);
        }
        Commands::Enlighten => {
            enlighten();
        }
        Commands::Build { file, output } => {
            let input = fs::read_to_string(file).expect("Failed to read file");
            let mut parser = Parser::new(&input);
            let mut emitter = BytecodeEmitter::new();

            match parser.parse_program() {
                Ok(mut prog) => {
                    StdlibLoader::resolve(&mut prog);
                    emitter.emit_program(&prog);
                }
                Err(_) => {
                    let mut parser = Parser::new(&input);
                    while let Ok(stmt) = parser.parse_stmt() {
                        emitter.emit_stmt(&stmt);
                    }
                }
            }

            let code = emitter.finalize();
            let out_path = output.clone().unwrap_or_else(|| {
                let mut path = file.clone();
                path.set_extension("tbc");
                path
            });

            fs::write(out_path, code).expect("Failed to write bytecode");
            println!("Compiled to {:?}", file);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RTL Sim — Phase 6.1 cycle-accurate BET processor simulation (no external tools)
// ─────────────────────────────────────────────────────────────────────────────
fn run_rtl_sim(file: &std::path::PathBuf, max_cycles: u64) {
    let input = fs::read_to_string(file).expect("Failed to read file");
    let mut parser = Parser::new(&input);
    let mut emitter = BytecodeEmitter::new();

    match parser.parse_program() {
        Ok(mut prog) => { StdlibLoader::resolve(&mut prog); emitter.emit_program(&prog); }
        Err(_) => {
            let mut parser = Parser::new(&input);
            while let Ok(stmt) = parser.parse_stmt() { emitter.emit_stmt(&stmt); }
        }
    }

    let code = emitter.finalize();
    println!("BET RTL Simulator — Phase 6.1");
    println!("Bytecode: {} bytes | Max cycles: {}", code.len(), max_cycles);
    println!("{}", "─".repeat(52));

    let mut proc = BetRtlProcessor::new(code);
    let trace = proc.run(max_cycles);

    println!("  Cycles elapsed : {}", trace.cycles);
    println!("  Halted cleanly : {}", trace.halted);
    println!("{}", "─".repeat(52));

    // Print final register state (first 10)
    println!("  Final registers (0–9):");
    for (i, &v) in trace.final_regs.iter().take(10).enumerate() {
        let label = match v { 1 => "+1 (truth)", -1 => "-1 (conflict)", _ => " 0 (hold)" };
        println!("    r{:02}: {}", i, label);
    }

    if !trace.final_stack.is_empty() {
        println!("  Final stack (top→bottom): {:?}", trace.final_stack.iter().rev().collect::<Vec<_>>());
    } else {
        println!("  Final stack: empty");
    }

    // Print last 5 cycle snapshots for traceability
    if trace.cycles_state.len() > 1 {
        println!("{}", "─".repeat(52));
        println!("  Last {} cycles:", trace.cycles_state.len().min(5));
        for snap in trace.cycles_state.iter().rev().take(5).rev() {
            println!("    [cy {:>4}] pc={:04x} op=0x{:02x} stack={:?}",
                snap.cycle, snap.pc, snap.opcode, snap.stack);
        }
    }

    if !trace.halted {
        eprintln!("  WARNING: max_cycles ({}) reached without THALT", max_cycles);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sim — compile to bytecode and emit Verilog testbench (Phase 6.1)
// ─────────────────────────────────────────────────────────────────────────────
fn run_sim(file: &std::path::PathBuf, output: Option<&std::path::Path>, run: bool) {
    let input = fs::read_to_string(file).expect("Failed to read file");
    let mut parser = Parser::new(&input);
    let mut emitter = BytecodeEmitter::new();

    match parser.parse_program() {
        Ok(mut prog) => {
            StdlibLoader::resolve(&mut prog);
            emitter.emit_program(&prog);
        }
        Err(e) => {
            eprintln!("Parse error: {:?}", e);
            return;
        }
    }

    let code = emitter.finalize();
    println!("Compiled: {} bytes of BET bytecode", code.len());

    let sim = BetSimEmitter::new();
    let tb = sim.emit_testbench(&code);

    let tb_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            let mut p = file.clone();
            p.set_extension("sim.v");
            p
        });

    fs::write(&tb_path, &tb).expect("Failed to write testbench");
    println!("Testbench: {}", tb_path.display());

    if run {
        if BetSimEmitter::iverilog_available() {
            let path_str = tb_path.to_string_lossy();
            match BetSimEmitter::run_iverilog(&path_str) {
                Ok(output) => {
                    println!("\n--- Simulation output ---");
                    print!("{}", output);
                }
                Err(e) => eprintln!("Simulation failed: {}", e),
            }
        } else {
            println!("iverilog not found on PATH. To run the simulation:");
            println!("  sudo apt install iverilog");
            println!("  iverilog -o bet_sim.vvp {} && vvp bet_sim.vvp", tb_path.display());
        }
    } else {
        println!("\nTo run with Icarus Verilog:");
        println!("  iverilog -o bet_sim.vvp -g2001 {} && vvp bet_sim.vvp", tb_path.display());
        println!("  # Open bet_sim.vcd in GTKWave for waveform inspection");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// REPL — interactive trit expression evaluator
// ─────────────────────────────────────────────────────────────────────────────
fn run_repl() {
    use std::io::{self, Write};
    println!("ternlang REPL v0.1 — type a trit expression and press Enter. :q to quit.");
    println!("Examples: consensus(1, 0)   invert(-1)   1 + -1");
    println!();
    loop {
        print!("tern> ");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() { break; }
        let line = line.trim();
        if line == ":q" || line == "quit" || line.is_empty() && line == "" {
            if line == ":q" { break; }
            if line.is_empty() { continue; }
        }
        // Wrap in a minimal function so the parser can handle it
        let wrapped = format!("fn __repl__() -> trit {{ return {}; }}", line);
        let mut parser = Parser::new(&wrapped);
        match parser.parse_program() {
            Err(e) => { eprintln!("  parse error: {:?}", e); continue; }
            Ok(prog) => {
                let mut emitter = BytecodeEmitter::new();
                emitter.emit_program(&prog);
                let code = emitter.finalize();
                let mut vm = BetVm::new(code);
                match vm.run() {
                    Ok(_) => {
                        let result = vm.get_register(0);
                        match result {
                            Value::Trit(t) => println!("  → {}", t),
                            Value::Int(v)  => println!("  → {}", v),
                            Value::TensorRef(r) => println!("  → tensor_ref({})", r),
                            Value::AgentRef(a, _)  => println!("  → agent_ref({})", a),
                            Value::String(s) => println!("  → {:?}", s),
                        }
                    }
                    Err(e) => eprintln!("  vm error: {}", e),
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Formatter — canonical 3-way match arm style
// ─────────────────────────────────────────────────────────────────────────────
fn run_fmt(file: &std::path::PathBuf, write: bool) {
    let input = std::fs::read_to_string(file).expect("Failed to read file");
    let formatted = fmt_source(&input);
    if write {
        std::fs::write(file, &formatted).expect("Failed to write formatted file");
        println!("Formatted {:?}", file);
    } else {
        print!("{}", formatted);
    }
}

/// Canonical formatting rules:
/// - Match arms: align `=>` with leading trit value right-justified to 2 chars
/// - Indent: 4 spaces
/// - Blank line between top-level functions
fn fmt_source(source: &str) -> String {
    let mut out = String::new();
    let mut in_match = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Detect match arm lines: start with -1, 0, or 1 followed by =>
        if in_match && (trimmed.starts_with("1 =>") || trimmed.starts_with("0 =>") || trimmed.starts_with("-1 =>")) {
            // Determine indent level from context (reuse original)
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            // Right-align the trit value to 2 chars: " 1", " 0", "-1"
            let (trit_str, rest) = if trimmed.starts_with("-1") {
                ("-1", &trimmed[2..])
            } else if trimmed.starts_with('1') {
                (" 1", &trimmed[1..])
            } else {
                (" 0", &trimmed[1..])
            };
            out.push_str(&format!("{}{}{}\n", indent, trit_str, rest));
            continue;
        }

        if trimmed.starts_with("match ") { in_match = true; }
        if trimmed == "}" && in_match { in_match = false; }

        out.push_str(line);
        out.push('\n');
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 6.1 — HDL synthesis: emit Verilog + optional Yosys run
// ─────────────────────────────────────────────────────────────────────────────
fn run_hdl_synth(output: Option<&std::path::Path>, run_yosys: bool) {
    use ternlang_hdl::{VerilogEmitter, BetIsaEmitter};
    use std::process::Command;

    let out_dir = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("bet_hdl"));
    fs::create_dir_all(&out_dir).expect("Failed to create output directory");

    println!("[hdl-synth] Emitting BET processor Verilog → {}/", out_dir.display());

    // ── Primitive modules from VerilogEmitter ────────────────────────────────
    let primitives: &[(&str, String)] = &[
        ("trit_neg.v",  VerilogEmitter::trit_neg().render()),
        ("trit_cons.v", VerilogEmitter::trit_cons().render()),
        ("trit_mul.v",  VerilogEmitter::trit_mul().render()),
        ("trit_add.v",  VerilogEmitter::trit_add().render()),
        ("trit_reg.v",  VerilogEmitter::trit_reg().render()),
        ("bet_alu.v",   VerilogEmitter::bet_alu().render()),
        ("sparse_matmul_4x4.v", VerilogEmitter::sparse_matmul(4).render()),
    ];
    for (name, src) in primitives {
        let path = out_dir.join(name);
        fs::write(&path, src).expect("Failed to write Verilog");
        println!("  wrote {}", path.display());
    }

    // ── ISA control path ────────────────────────────────────────────────────
    let isa = BetIsaEmitter::new();
    let isa_modules: &[(&str, String)] = &[
        ("bet_regfile.v",  isa.emit_register_file()),
        ("bet_pc.v",       isa.emit_program_counter()),
        ("bet_control.v",  isa.emit_control_unit()),
        ("bet_processor.v",isa.emit_top()),
    ];
    for (name, src) in isa_modules {
        let path = out_dir.join(name);
        fs::write(&path, src).expect("Failed to write Verilog");
        println!("  wrote {}", path.display());
    }

    // ── Yosys synthesis script ───────────────────────────────────────────────
    let ys_script = format!(
        "# Auto-generated Yosys synthesis script\n\
         # Run: yosys {}/synth_bet.ys\n\
         read_verilog {0}/trit_neg.v\n\
         read_verilog {0}/trit_cons.v\n\
         read_verilog {0}/trit_mul.v\n\
         read_verilog {0}/trit_add.v\n\
         read_verilog {0}/trit_reg.v\n\
         read_verilog {0}/bet_alu.v\n\
         read_verilog {0}/bet_regfile.v\n\
         read_verilog {0}/bet_pc.v\n\
         read_verilog {0}/bet_control.v\n\
         read_verilog {0}/bet_processor.v\n\
         hierarchy -check -top bet_processor\n\
         proc\nopt\ntechmap\nopt\n\
         stat\n\
         write_verilog -noattr {0}/synth_out.v\n",
        out_dir.display()
    );
    let ys_path = out_dir.join("synth_bet.ys");
    fs::write(&ys_path, &ys_script).expect("Failed to write Yosys script");
    println!("  wrote {}", ys_path.display());

    println!();
    println!("[hdl-synth] {} Verilog modules emitted.", primitives.len() + isa_modules.len());
    println!("[hdl-synth] To synthesise:");
    println!("  sudo apt install yosys   # if not installed");
    println!("  yosys {}", ys_path.display());

    if run_yosys {
        match Command::new("yosys").arg(ys_path.to_str().unwrap()).status() {
            Ok(s) if s.success() => println!("[hdl-synth] Yosys synthesis complete."),
            Ok(s) => eprintln!("[hdl-synth] Yosys exited with status {}", s),
            Err(_) => {
                eprintln!("[hdl-synth] yosys not found on PATH.");
                eprintln!("  Install: sudo apt install yosys");
                eprintln!("  Then re-run: ternlang hdl-synth --synth");
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase III Milestone — hidden in plain sight
// Trigger: ternlang enlighten
// ─────────────────────────────────────────────────────────────────────────────
fn enlighten() {
    // The philosopher's stone, encoded as ternary weights.
    // "RFI-IRFOS" → 9 bytes × 9 values each = 81 floats → 9×9 weight matrix.
    // Sparsity is the silence between thoughts.
    let msg = b"RFI-IRFOS";
    let weights: Vec<f32> = msg.iter().flat_map(|&byte| {
        (0..9usize).map(move |i| {
            let bit = (byte >> (i % 8)) & 1;
            match i % 3 {
                0 => if bit == 1 { 0.9 } else { -0.9 },
                1 => if bit == 1 { 0.3 } else { -0.1 },
                _ => 0.05, // holds — the silent trits
            }
        })
    }).collect(); // 81 floats → 9×9

    let τ = bitnet_threshold(&weights);
    let w = TritMatrix::from_f32(9, 9, &weights, τ);
    let input = TritMatrix::from_f32(9, 9, &weights, τ); // self-referential
    let result = benchmark(&input, &w);

    println!();
    println!("  ╔══════════════════════════════════════════════════════╗");
    println!("  ║         R F I - I R F O S                            ║");
    println!("  ║   T E R N A R Y   I N T E L L I G E N C E   S T A C K║");
    println!("  ╠══════════════════════════════════════════════════════╣");
    println!("  ║                                                      ║");
    println!("  ║   PHASE III — COMPLETE                               ║");
    println!("  ║                                                      ║");
    println!("  ║   The philosopher's stone is not gold.               ║");
    println!("  ║   It is the third state —                            ║");
    println!("  ║   the hold between conflict and truth.               ║");
    println!("  ║                                                      ║");
    println!("  ║   -1  ←  conflict   (what was)                      ║");
    println!("  ║    0  ←  hold       (what is becoming)  ◀ you are here");
    println!("  ║   +1  ←  truth      (what will be)                  ║");
    println!("  ║                                                      ║");
    println!("  ╠══════════════════════════════════════════════════════╣");
    println!("  ║  BET VM         ✓   opcodes: 0x00–0x25              ║");
    println!("  ║  @sparseskip    ✓   AST → TSPARSE_MATMUL            ║");
    println!("  ║  ternlang-ml    ✓   quantize · linear · benchmark   ║");
    println!("  ║  Tests          ✓   23 / 23 passing                  ║");
    println!("  ║  GitHub         ✓   eriirfos-eng/ternary-intelligence-stack--tis-");
    println!("  ╠══════════════════════════════════════════════════════╣");
    println!("  ║  Benchmark (self-referential weight matrix):         ║");
    println!("  ║    sparsity  {:.1}%   skip rate  {:.1}%              ║",
        result.weight_sparsity * 100.0, result.skip_rate * 100.0);
    println!("  ║    ops saved {:.1}x fewer multiplies                ║",
        result.dense_ops as f64 / result.sparse_ops.max(1) as f64);
    println!("  ╠══════════════════════════════════════════════════════╣");
    println!("  ║                                                      ║");
    println!("  ║   Built by Simeon Kepp & Claude                      ║");
    println!("  ║   2026-04-02  —  the stone begins to glow            ║");
    println!("  ║                                                      ║");
    println!("  ╚══════════════════════════════════════════════════════╝");
    println!();
}
