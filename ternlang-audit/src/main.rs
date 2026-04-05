use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};
use colored::*;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

/// Ternlang Decision Auditor — Observability & Resolution for Triadic Logic
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Analyze a deliberation trace and explain the decision
    Analyze {
        /// Path to the JSON trace file
        #[arg(short, long)]
        trace: PathBuf,
    },
    /// Compare two conflicting traces and suggest an arbitration path
    Resolve {
        /// First trace path
        #[arg(long)]
        a: PathBuf,
        /// Second trace path
        #[arg(long)]
        b: PathBuf,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentVerdictTrace {
    pub name: String,
    pub trit: i8,
    pub confidence: f32,
    pub reasoning: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeliberationTrace {
    pub timestamp: DateTime<Utc>,
    pub query: String,
    pub final_trit: i8,
    pub confidence: f32,
    pub agents: Vec<AgentVerdictTrace>,
    pub is_stable_hold: bool,
    pub veto_triggered: bool,
    pub veto_expert: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze { trace } => {
            let data = fs::read_to_string(trace)?;
            let t: DeliberationTrace = serde_json::from_str(&data)?;
            render_audit(&t);
        }
        Commands::Resolve { a, b } => {
            let trace_a: DeliberationTrace = serde_json::from_str(&fs::read_to_string(a)?)?;
            let trace_b: DeliberationTrace = serde_json::from_str(&fs::read_to_string(b)?)?;
            run_resolution(trace_a, trace_b);
        }
    }

    Ok(())
}

fn render_audit(t: &DeliberationTrace) {
    println!("\n{}", "=== TERNARY DECISION AUDIT ===".bold().cyan());
    println!("{:<15} {}", "Query:".dimmed(), t.query.white());
    println!("{:<15} {}", "Timestamp:".dimmed(), t.timestamp);
    
    let state_str = match t.final_trit {
        1 => "AFFIRM (+1)".green().bold(),
        -1 => "REJECT (-1)".red().bold(),
        _ => "TEND (0)".yellow().bold(),
    };
    println!("{:<15} {}", "Verdict:".dimmed(), state_str);
    println!("{:<15} {:.2}%", "Confidence:".dimmed(), t.confidence * 100.0);

    println!("\n{}", "--- Cause Chain ---".bold());
    if t.veto_triggered {
        println!("{} {} Expert vetoed the action.", "⚠ HARD VETO:".red().bold(), t.veto_expert.as_deref().unwrap_or("Unknown"));
    } else if t.is_stable_hold {
        println!("{} Signals are in equilibrium (Stable Attractor).", "⏸ STABLE HOLD:".yellow().bold());
    }

    let affirm_count = t.agents.iter().filter(|a| a.trit == 1).count();
    let reject_count = t.agents.iter().filter(|a| a.trit == -1).count();
    let hold_count = t.agents.iter().filter(|a| a.trit == 0).count();

    println!("Agent signals: {} Affirm, {} Reject, {} Hold", 
        affirm_count.to_string().green(), 
        reject_count.to_string().red(), 
        hold_count.to_string().yellow()
    );

    println!("\n{}", "--- Agent Details ---".bold());
    for agent in &t.agents {
        let a_trit = match agent.trit {
            1 => "+1".green(),
            -1 => "-1".red(),
            _ => " 0".yellow(),
        };
        println!("  [{}] {:<15} | conf: {:.2} | {}", a_trit, agent.name.cyan(), agent.confidence, agent.reasoning.dimmed());
    }
    println!("");
}

fn run_resolution(a: DeliberationTrace, b: DeliberationTrace) {
    println!("\n{}", "=== CONFLICT RESOLUTION ENGINE ===".bold().magenta());
    println!("Conflict detected between two deliberation cycles.\n");
    
    println!("Trace A: {} (trit: {})", a.query.dimmed(), a.final_trit);
    println!("Trace B: {} (trit: {})", b.query.dimmed(), b.final_trit);

    if a.final_trit != b.final_trit {
        println!("\n{}", "Arbitration Path:".bold());
        if a.veto_triggered || b.veto_triggered {
            println!("  {} Safety veto detected in one trace. Blocking entire chain.", "•".red());
            println!("  Recommended Trit: {}", "-1 (REJECT)".red().bold());
        } else {
            println!("  {} Opposing non-veto signals detected.", "•".yellow());
            println!("  Initiating 'Ternary Consensus Protocol'...");
            println!("  Recommended Trit: {}", "0 (TEND)".yellow().bold());
            println!("  Reason: System requires updated evidence vector to break equilibrium.");
        }
    } else {
        println!("\nNo logical conflict detected. Traces are aligned.");
    }
    println!("");
}
