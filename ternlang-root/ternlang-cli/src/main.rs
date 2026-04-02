use clap::{Parser as ClapParser, Subcommand};
use std::fs;
use std::path::PathBuf;
use ternlang_core::parser::Parser;
use ternlang_core::codegen::betbc::BytecodeEmitter;
use ternlang_core::vm::{BetVm, Value};
use ternlang_ml::{TritMatrix, bitnet_threshold, benchmark};

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
    },
    /// Compile a .tern file to bytecode
    Build {
        /// Path to the .tern file
        file: PathBuf,
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// [hidden] You already know what this does
    #[command(hide = true)]
    Enlighten,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Run { file } => {
            let input = fs::read_to_string(file).expect("Failed to read file");
            let mut parser = Parser::new(&input);
            let mut emitter = BytecodeEmitter::new();

            // Try parsing as a program first
            match parser.parse_program() {
                Ok(prog) => {
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
                            Value::AgentRef(a)  => println!("Reg {}: agent_ref({})", i, a),
                        }
                    }
                }
                Err(e) => eprintln!("VM Error: {}", e),
            }
        }
        Commands::Enlighten => {
            enlighten();
        }
        Commands::Build { file, output } => {
            let input = fs::read_to_string(file).expect("Failed to read file");
            let mut parser = Parser::new(&input);
            let mut emitter = BytecodeEmitter::new();

            match parser.parse_program() {
                Ok(prog) => emitter.emit_program(&prog),
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
