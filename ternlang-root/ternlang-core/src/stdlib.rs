/// StdlibLoader — resolves `use` statements into parsed function definitions.
///
/// When user code contains `use std::trit;` inside a function body, this module
/// parses the corresponding stdlib source and injects the functions into the
/// program before semantic analysis and codegen.
///
/// Stdlib sources are embedded at compile time via `include_str!` so the
/// compiler binary is fully self-contained — no filesystem lookups at runtime.
use crate::ast::{Program, Stmt};
use crate::parser::Parser;

pub struct StdlibLoader;

impl StdlibLoader {
    fn source_for(path: &[String]) -> Option<&'static str> {
        match path.join("::").as_str() {
            "std::trit"     => Some(include_str!("../../stdlib/std/trit.tern")),
            "std::math"     => Some(include_str!("../../stdlib/std/math.tern")),
            "std::tensor"   => Some(include_str!("../../stdlib/std/tensor.tern")),
            "std::io"       => Some(include_str!("../../stdlib/std/io.tern")),
            "ml::quantize"  => Some(include_str!("../../stdlib/ml/quantize.tern")),
            "ml::inference" => Some(include_str!("../../stdlib/ml/inference.tern")),
            _               => None,
        }
    }

    /// Recursively collect `use` paths from a slice of statements.
    fn collect_use_paths(stmts: &[Stmt]) -> Vec<Vec<String>> {
        let mut paths = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::Use { path } => paths.push(path.clone()),
                Stmt::Block(inner) => paths.extend(Self::collect_use_paths(inner)),
                Stmt::IfTernary { on_pos, on_zero, on_neg, .. } => {
                    paths.extend(Self::collect_use_paths(&[*on_pos.clone()]));
                    paths.extend(Self::collect_use_paths(&[*on_zero.clone()]));
                    paths.extend(Self::collect_use_paths(&[*on_neg.clone()]));
                }
                Stmt::Match { arms, .. } => {
                    for (_, arm_stmt) in arms {
                        paths.extend(Self::collect_use_paths(&[arm_stmt.clone()]));
                    }
                }
                _ => {}
            }
        }
        paths
    }

    /// Parse stdlib modules referenced by `use` statements and prepend their
    /// functions to `program.functions`.  Functions already present by name are
    /// not duplicated, so calling this multiple times is safe.
    pub fn resolve(program: &mut Program) {
        // Build the set of already-defined function names
        let mut known: std::collections::HashSet<String> =
            program.functions.iter().map(|f| f.name.clone()).collect();

        // Collect all use paths from every function body
        let mut all_paths: Vec<Vec<String>> = program
            .functions
            .iter()
            .flat_map(|f| Self::collect_use_paths(&f.body))
            .collect();

        // Deduplicate so we parse each module at most once
        all_paths.sort();
        all_paths.dedup();

        let mut stdlib_fns = Vec::new();

        for path in &all_paths {
            let key = path.join("::");
            let Some(src) = Self::source_for(path) else {
                // Unknown module — leave as-is; semantic checker will surface errors
                continue;
            };
            let mut parser = Parser::new(src);
            match parser.parse_program() {
                Ok(stdlib_prog) => {
                    for f in stdlib_prog.functions {
                        if !known.contains(&f.name) {
                            known.insert(f.name.clone());
                            stdlib_fns.push(f);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[stdlib] Failed to parse {}: {:?}", key, e);
                }
            }
        }

        // Prepend stdlib functions so they appear before user-defined functions
        // (call order in bytecode doesn't matter, but it keeps the symbol table tidy)
        stdlib_fns.extend(program.functions.drain(..));
        program.functions = stdlib_fns;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    /// Verify that each stdlib module parses without errors.
    #[test]
    fn all_stdlib_modules_parse() {
        let modules = [
            vec!["std".to_string(), "trit".to_string()],
            vec!["std".to_string(), "math".to_string()],
            vec!["std".to_string(), "tensor".to_string()],
            vec!["std".to_string(), "io".to_string()],
            vec!["ml".to_string(), "quantize".to_string()],
            vec!["ml".to_string(), "inference".to_string()],
        ];
        for path in &modules {
            let src = StdlibLoader::source_for(path)
                .unwrap_or_else(|| panic!("Missing stdlib source for {}", path.join("::")));
            let mut parser = Parser::new(src);
            parser.parse_program()
                .unwrap_or_else(|e| panic!("Parse error in {}: {:?}", path.join("::"), e));
        }
    }

    /// A program with `use std::trit;` should gain abs/min/max/etc after resolve.
    #[test]
    fn resolve_injects_trit_stdlib() {
        let src = r#"
fn main() -> trit {
    use std::trit;
    let x: trit = abs(-1);
    return x;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        assert!(!prog.functions.iter().any(|f| f.name == "abs"),
            "abs should not be present before resolve");
        StdlibLoader::resolve(&mut prog);
        assert!(prog.functions.iter().any(|f| f.name == "abs"),
            "abs should be injected after resolve");
        assert!(prog.functions.iter().any(|f| f.name == "min"));
        assert!(prog.functions.iter().any(|f| f.name == "majority"));
    }

    /// Multiple use statements should all be resolved, with no duplicates.
    #[test]
    fn resolve_multiple_modules_no_duplicates() {
        let src = r#"
fn main() -> trit {
    use std::trit;
    use std::math;
    let x: trit = neg(1);
    return x;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        StdlibLoader::resolve(&mut prog);

        // Count how many times "neg" appears — should be exactly 1
        let neg_count = prog.functions.iter().filter(|f| f.name == "neg").count();
        assert_eq!(neg_count, 1, "neg should appear exactly once");

        // Both modules should be present
        assert!(prog.functions.iter().any(|f| f.name == "abs"));   // std::trit
        assert!(prog.functions.iter().any(|f| f.name == "rectify")); // std::math
    }

    /// Resolve is idempotent — calling it twice should not duplicate functions.
    #[test]
    fn resolve_is_idempotent() {
        let src = r#"
fn main() -> trit {
    use std::trit;
    return 0;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        StdlibLoader::resolve(&mut prog);
        StdlibLoader::resolve(&mut prog);
        let abs_count = prog.functions.iter().filter(|f| f.name == "abs").count();
        assert_eq!(abs_count, 1, "abs should not be duplicated by double resolve");
    }

    /// Unknown module paths are silently skipped (not a hard error).
    #[test]
    fn unknown_module_skipped_gracefully() {
        let src = r#"
fn main() -> trit {
    use std::nonexistent;
    return 0;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        // Should not panic
        StdlibLoader::resolve(&mut prog);
    }
}
