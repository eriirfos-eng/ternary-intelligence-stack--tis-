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

    let rules = vec![
        TranslationRule {
            pattern: Regex::new(r"bool").unwrap(),
            replacement: "trit",
            note: "Mapped binary bool to triadic trit.",
        },
        TranslationRule {
            pattern: Regex::new(r"true").unwrap(),
            replacement: "affirm",
            note: "Mapped true to +1 (affirm).",
        },
        TranslationRule {
            pattern: Regex::new(r"false").unwrap(),
            replacement: "reject",
            note: "Mapped false to -1 (reject).",
        },
        TranslationRule {
            pattern: Regex::new(r"None|null|undefined").unwrap(),
            replacement: "tend",
            note: "Mapped null/empty to 0 (tend).",
        },
        TranslationRule {
            pattern: Regex::new(r"if\s*\((.*)\)\s*\{").unwrap(),
            replacement: "match $1 {",
            note: "Converted if-statement to exhaustive match block.",
        },
        TranslationRule {
            pattern: Regex::new(r"if\s+(.*):").unwrap(),
            replacement: "match $1 {",
            note: "Python-style if converted to match.",
        },
        TranslationRule {
            pattern: Regex::new(r"else\s*\{|else:").unwrap(),
            replacement: "tend => { // auto-injected safety hold\n    }\n    reject => {",
            note: "Injected state 0 (tend) for logic exhaustiveness.",
        },
        TranslationRule {
            pattern: Regex::new(r"(?i)return\s+1|return\s+True").unwrap(),
            replacement: "return affirm",
            note: "Normalised positive return.",
        },
        TranslationRule {
            pattern: Regex::new(r"(?i)return\s+0|return\s+False").unwrap(),
            replacement: "return reject",
            note: "Normalised negative return.",
        },
        TranslationRule {
            pattern: Regex::new(r"void|def|fn").unwrap(),
            replacement: "fn",
            note: "Unified function declaration.",
        },
    ];

    for rule in rules {
        if rule.pattern.is_match(&translated) {
            translated = rule.pattern.replace_all(&translated, rule.replacement).to_string();
            insights.push(rule.note);
        }
    }

    // Add header
    if !translated.starts_with("//") {
        translated = format!("// Translated to Ternlang from {:?}\n\n{}", args.input, translated);
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
