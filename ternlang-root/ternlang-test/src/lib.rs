// SPDX-License-Identifier: LGPL-3.0-or-later
// Ternlang — RFI-IRFOS Ternary Intelligence Stack
// Copyright (C) 2026 RFI-IRFOS
// Open-core compiler. See LICENSE-LGPL in the repository root.

//! ternlang-test — test framework for `.tern` programs.
//!
//! Runs source strings through the full pipeline (parse → stdlib resolve →
//! semantic check → codegen → BET VM) and asserts on the outcome.
//!
//! # Quick start
//! ```rust,no_run
//! # use ternlang_test::{TernTestCase, TernExpected, assert_tern};
//! # fn main() {
//! assert_tern!(TernTestCase {
//!     name: "hold is the zero state",
//!     source: "fn main() -> trit { return 0; }",
//!     expected: TernExpected::Trit(0),
//! });
//! # }
//! ```

use ternlang_core::{
    Parser, SemanticAnalyzer, BytecodeEmitter, StdlibLoader, BetVm,
    vm::Value,
    trit::Trit,
};

// ─── Test case definition ─────────────────────────────────────────────────────

/// What a test expects as its outcome.
#[derive(Debug, Clone)]
pub enum TernExpected {
    /// The program's return value should equal this trit (-1, 0, or +1).
    Trit(i8),
    /// The program should fail to parse (any parse error satisfies this).
    ParseError,
    /// The program should pass parsing but fail semantic analysis.
    SemanticError,
}

/// A single ternlang test case.
pub struct TernTestCase {
    pub name: &'static str,
    /// Complete `.tern` source. Must contain a `fn main() -> trit { }`.
    pub source: &'static str,
    pub expected: TernExpected,
}

// ─── Result type ─────────────────────────────────────────────────────────────

/// Outcome of running a `TernTestCase`.
#[derive(Debug)]
pub struct TernTestResult {
    pub name: &'static str,
    pub passed: bool,
    pub actual_trit: Option<i8>,
    pub message: String,
}

impl TernTestResult {
    fn pass(name: &'static str, trit: Option<i8>) -> Self {
        Self { name, passed: true, actual_trit: trit, message: "ok".into() }
    }

    fn fail(name: &'static str, msg: impl Into<String>) -> Self {
        Self { name, passed: false, actual_trit: None, message: msg.into() }
    }

    fn fail_trit(name: &'static str, actual: i8, msg: impl Into<String>) -> Self {
        Self { name, passed: false, actual_trit: Some(actual), message: msg.into() }
    }
}

// ─── Runner ──────────────────────────────────────────────────────────────────

/// Run a single test case through the complete ternlang pipeline and return
/// a result indicating pass/fail with a diagnostic message.
pub fn run_tern_test(case: &TernTestCase) -> TernTestResult {
    // 1. Parse
    let mut parser = Parser::new(case.source);
    let prog = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("{}", e);
            let passed = matches!(case.expected, TernExpected::ParseError);
            return if passed {
                TernTestResult::pass(case.name, None)
            } else {
                TernTestResult::fail(case.name, format!("Parse error (unexpected): {msg}"))
            };
        }
    };
    if matches!(case.expected, TernExpected::ParseError) {
        return TernTestResult::fail(case.name, "expected a parse error but program parsed successfully");
    }

    // 2. Stdlib resolve
    let mut prog = prog;
    StdlibLoader::resolve(&mut prog);

    // 3. Semantic analysis
    let mut checker = SemanticAnalyzer::new();
    if let Err(e) = checker.check_program(&prog) {
        let msg = format!("{}", e);
        let passed = matches!(case.expected, TernExpected::SemanticError);
        return if passed {
            TernTestResult::pass(case.name, None)
        } else {
            TernTestResult::fail(case.name, format!("Semantic error (unexpected): {msg}"))
        };
    }
    if matches!(case.expected, TernExpected::SemanticError) {
        return TernTestResult::fail(case.name, "expected a semantic error but program passed analysis");
    }

    // 4. Codegen
    let mut emitter = BytecodeEmitter::new();
    emitter.emit_program(&prog);
    // Emit a TCALL to main so the entry TJMP lands on the actual invocation.
    emitter.emit_entry_call("main");
    let code = emitter.finalize();

    // 5. VM execution
    let mut vm = BetVm::new(code);
    match vm.run() {
        Err(e) => TernTestResult::fail(case.name, format!("VM error: {e}")),
        Ok(()) => {
            // Result is the top of the stack after execution.
            let trit_val: i8 = match vm.peek_stack() {
                Some(Value::Trit(Trit::Affirm))  =>  1,
                Some(Value::Trit(Trit::Tend))    =>  0,
                Some(Value::Trit(Trit::Reject))  => -1,
                Some(other) => {
                    return TernTestResult::fail(
                        case.name,
                        format!("VM returned non-trit value: {other:?}"),
                    );
                }
                None => {
                    return TernTestResult::fail(case.name, "VM stack is empty after execution");
                }
            };

            match &case.expected {
                TernExpected::Trit(expected) => {
                    if trit_val == *expected {
                        TernTestResult::pass(case.name, Some(trit_val))
                    } else {
                        TernTestResult::fail_trit(
                            case.name,
                            trit_val,
                            format!("expected trit={expected}, got trit={trit_val}"),
                        )
                    }
                }
                // Already handled above
                TernExpected::ParseError | TernExpected::SemanticError => unreachable!(),
            }
        }
    }
}

// ─── Assertion macro ──────────────────────────────────────────────────────────

/// Run a `TernTestCase` and panic with a readable message if it fails.
///
/// ```rust,no_run
/// # use ternlang_test::{TernTestCase, TernExpected, assert_tern};
/// # fn main() {
/// assert_tern!(TernTestCase {
///     name: "hold is zero",
///     source: "fn main() -> trit { return 0; }",
///     expected: TernExpected::Trit(0),
/// });
/// # }
/// ```
#[macro_export]
macro_rules! assert_tern {
    ($case:expr) => {{
        let result = $crate::run_tern_test(&$case);
        assert!(
            result.passed,
            "\n[TERN-TEST] '{}' failed\n  → {}\n",
            result.name,
            result.message,
        );
        result
    }};
}

// ─── Built-in test suite ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &'static str, source: &'static str, expected: TernExpected) -> TernTestResult {
        run_tern_test(&TernTestCase { name, source, expected })
    }

    // ── Trit literals ─────────────────────────────────────────────────────────

    #[test]
    fn trit_pos_literal() {
        let r = run("trit +1", "fn main() -> trit { return 1; }", TernExpected::Trit(1));
        assert!(r.passed, "{}", r.message);
    }

    #[test]
    fn trit_zero_literal() {
        let r = run("trit 0", "fn main() -> trit { return 0; }", TernExpected::Trit(0));
        assert!(r.passed, "{}", r.message);
    }

    #[test]
    fn trit_neg_literal() {
        let r = run("trit -1", "fn main() -> trit { return -1; }", TernExpected::Trit(-1));
        assert!(r.passed, "{}", r.message);
    }

    // ── Consensus ─────────────────────────────────────────────────────────────

    #[test]
    fn consensus_pos_and_zero() {
        // balanced ternary: 1 + 0 = 1
        let r = run(
            "consensus(+1, 0)=+1",
            "fn main() -> trit { return consensus(1, 0); }",
            TernExpected::Trit(1),
        );
        assert!(r.passed, "{}", r.message);
    }

    #[test]
    fn consensus_conflict_holds() {
        // balanced ternary: 1 + (-1) = 0
        let r = run(
            "consensus(+1,-1)=0",
            "fn main() -> trit { return consensus(1, -1); }",
            TernExpected::Trit(0),
        );
        assert!(r.passed, "{}", r.message);
    }

    // ── Error propagation (`?`) ───────────────────────────────────────────────

    #[test]
    fn propagate_passes_through_pos() {
        let r = run(
            "propagate pass-through on +1",
            r#"
fn check() -> trit { return 1; }
fn main() -> trit { return check()?; }
"#,
            TernExpected::Trit(1),
        );
        assert!(r.passed, "{}", r.message);
    }

    #[test]
    fn propagate_early_returns_on_neg() {
        // check() returns -1 → main() should return -1 via propagation
        let r = run(
            "propagate early return on -1",
            r#"
fn check() -> trit { return -1; }
fn main() -> trit {
    let x: trit = check()?;
    return 1;
}
"#,
            TernExpected::Trit(-1),
        );
        assert!(r.passed, "{}", r.message);
    }

    // ── Module system ─────────────────────────────────────────────────────────

    #[test]
    fn stdlib_trit_resolves() {
        let r = run(
            "std::trit resolves",
            r#"
fn main() -> trit {
    use std::trit;
    return abs(-1);
}
"#,
            TernExpected::Trit(1),
        );
        assert!(r.passed, "{}", r.message);
    }

    // ── Error messages ────────────────────────────────────────────────────────

    #[test]
    fn non_exhaustive_match_is_parse_error() {
        let r = run(
            "non-exhaustive match",
            r#"
fn main() -> trit {
    let x: trit = 1;
    match x { 1 => { return 1; } 0 => { return 0; } }
}
"#,
            TernExpected::ParseError,
        );
        assert!(r.passed, "{}", r.message);
    }

    #[test]
    fn undefined_variable_is_semantic_error() {
        let r = run(
            "undefined variable",
            "fn main() -> trit { return ghost; }",
            TernExpected::SemanticError,
        );
        assert!(r.passed, "{}", r.message);
    }
}
