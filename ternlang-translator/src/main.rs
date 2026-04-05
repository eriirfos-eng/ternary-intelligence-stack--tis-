use clap::Parser;
use regex::Regex;
use colored::*;
use std::fs;
use std::path::PathBuf;

/// TernTranslator — Migrate binary logic to balanced ternary (.tern)
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file path (e.g. logic.py, safety.rs)
    #[arg(short, long)]
    input: PathBuf,

    /// Output file path (defaults to <input>.tern)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Print detailed triadic insights
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Clone)]
struct TranslationRule {
    pattern: Regex,
    replacement: &'static str,
    note: &'static str,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let input_code = fs::read_to_string(&args.input)?;
    let mut translated = input_code.clone();
    let mut insights = Vec::new();

    // ── Phase 1: Comparison & Logic (Highest Priority) ──────────────────────
    let logic_rules = vec![
        TranslationRule {
            pattern: Regex::new(r"==\s*true|==\s*True").unwrap(),
            replacement: " is affirm",
            note: "Converted boolean comparison to triadic 'is' match.",
        },
        TranslationRule {
            pattern: Regex::new(r"==\s*false|==\s*False").unwrap(),
            replacement: " is reject",
            note: "Converted boolean comparison to triadic 'is' match.",
        },
        TranslationRule {
            pattern: Regex::new(r"is\s+None|==\s*null").unwrap(),
            replacement: " is tend",
            note: "Converted null check to state 0 (tend) check.",
        },
        TranslationRule {
            pattern: Regex::new(r"\band\b|&&").unwrap(),
            replacement: " consensus ",
            note: "Mapped AND logic to ternary consensus.",
        },
        TranslationRule {
            pattern: Regex::new(r"\bor\b|\|\|").unwrap(),
            replacement: " any ",
            note: "Mapped OR logic to ternary any.",
        },
        TranslationRule {
            pattern: Regex::new(r"\bnot\b|!").unwrap(),
            replacement: "invert ",
            note: "Mapped NOT logic to trit inversion.",
        },
    ];

    // ── Phase 2: Structural Flow ────────────────────────────────────────────
    let flow_rules = vec![
        TranslationRule {
            pattern: Regex::new(r"elif\s+(.*):|else\s+if\s*\((.*)\)\s*\{").unwrap(),
            replacement: "    0 => { // cascading deliberation\n        match $1$2 {",
            note: "Refactored elif/else-if into nested deliberation arms.",
        },
        TranslationRule {
            pattern: Regex::new(r"if\s*\((.*)\)\s*\{|if\s+(.*):").unwrap(),
            replacement: "match $1$2 {",
            note: "Converted if-statement to exhaustive match block.",
        },
        TranslationRule {
            pattern: Regex::new(r"else\s*\{|else:").unwrap(),
            replacement: "tend => { // auto-injected safety hold\n    }\n    reject => {",
            note: "Injected state 0 (tend) for logic exhaustiveness.",
        },
    ];

    // ── Phase 3: Types & Values ─────────────────────────────────────────────
    let type_rules = vec![
        TranslationRule {
            pattern: Regex::new(r"\bbool\b").unwrap(),
            replacement: "trit",
            note: "Mapped binary bool to triadic trit.",
        },
        TranslationRule {
            pattern: Regex::new(r"\btrue\b|\bTrue\b").unwrap(),
            replacement: "affirm",
            note: "Mapped true to +1 (affirm).",
        },
        TranslationRule {
            pattern: Regex::new(r"\bfalse\b|\bFalse\b").unwrap(),
            replacement: "reject",
            note: "Mapped false to -1 (reject).",
        },
        TranslationRule {
            pattern: Regex::new(r"\bNone\b|\bnull\b|\bundefined\b").unwrap(),
            replacement: "tend",
            note: "Mapped null/empty to 0 (tend).",
        },
        TranslationRule {
            pattern: Regex::new(r"(?i)return\s+1|return\s+True|return\s+affirm").unwrap(),
            replacement: "return affirm",
            note: "Normalised positive return.",
        },
        TranslationRule {
            pattern: Regex::new(r"(?i)return\s+0|return\s+False|return\s+reject").unwrap(),
            replacement: "return reject",
            note: "Normalised negative return.",
        },
        TranslationRule {
            pattern: Regex::new(r"\bvoid\b|\bdef\b|\bfn\b").unwrap(),
            replacement: "fn",
            note: "Unified function declaration.",
        },
    ];

    let all_rules = [logic_rules, flow_rules, type_rules].concat();

    for rule in all_rules {
        if rule.pattern.is_match(&translated) {
            translated = rule.pattern.replace_all(&translated, rule.replacement).to_string();
            insights.push(rule.note);
        }
    }

    // Add header
    if !translated.starts_with("//") {
        translated = format!("// Translated to Ternlang from {:?}\n// Algorithm: Triadic Refactor v0.2.1\n\n{}", args.input, translated);
    }

    let output_path = args.output.unwrap_or_else(|| {
        let mut p = args.input.clone();
        p.set_extension("tern");
        p
    });

    fs::write(&output_path, translated)?;

    println!("{}", "✔ Translation complete!".green().bold());
    println!("{} {:?}", "Output written to:".dimmed(), output_path);

    if args.verbose || !insights.is_empty() {
        println!("\n{}", "Triadic Logic Insights:".cyan().bold());
        let mut unique_insights = insights.clone();
        unique_insights.sort();
        unique_insights.dedup();
        for note in unique_insights {
            println!("  {} {}", "•".cyan(), note);
        }
    }

    Ok(())
}
