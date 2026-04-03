//! Owlet S-expression front-end for ternlang
//!
//! Parses Owlet-style S-expression ternary programs into ternlang AST
//! nodes, which can then be compiled to BET bytecode and run on the VM.
//!
//! ## Owlet syntax
//! ```text
//! (+ 1 -1)              ; add two trits → consensus
//! (neg -1)              ; negate → +1
//! (cons 0 1)            ; consensus
//! (fn f (x) (neg x))   ; define function f
//! (f 1)                 ; call f(+1) → -1
//! ```
//!
//! All numbers in Owlet are signed by default. Only -1, 0, +1 are valid trit values.

use ternlang_core::ast::{Expr, Stmt, Function, Program, Type};

/// A parsed S-expression token tree.
#[derive(Debug, PartialEq, Clone)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

/// Parse an S-expression string into a `Sexp` tree.
pub fn parse_sexp(input: &str) -> Result<Sexp, String> {
    let tokens = tokenise(input);
    let mut pos = 0;
    parse_sexp_tokens(&tokens, &mut pos)
}

fn tokenise(input: &str) -> Vec<String> {
    let mut tokens  = Vec::new();
    let mut current = String::new();
    let mut in_comment = false;

    for ch in input.chars() {
        if in_comment {
            if ch == '\n' { in_comment = false; }
            continue;
        }
        match ch {
            ';' => {
                if !current.is_empty() { tokens.push(current.clone()); current.clear(); }
                in_comment = true;
            }
            '(' | ')' => {
                if !current.is_empty() { tokens.push(current.clone()); current.clear(); }
                tokens.push(ch.to_string());
            }
            ' ' | '\t' | '\n' | '\r' => {
                if !current.is_empty() { tokens.push(current.clone()); current.clear(); }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() { tokens.push(current); }
    tokens
}

fn parse_sexp_tokens(tokens: &[String], pos: &mut usize) -> Result<Sexp, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of input".to_string());
    }
    let tok = tokens[*pos].clone();
    *pos += 1;

    if tok == "(" {
        let mut list = Vec::new();
        while *pos < tokens.len() && tokens[*pos] != ")" {
            list.push(parse_sexp_tokens(tokens, pos)?);
        }
        if *pos >= tokens.len() {
            return Err("Unmatched '('".to_string());
        }
        *pos += 1; // consume ')'
        Ok(Sexp::List(list))
    } else if tok == ")" {
        Err("Unexpected ')'".to_string())
    } else {
        Ok(Sexp::Atom(tok))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sexp → ternlang AST
// ─────────────────────────────────────────────────────────────────────────────

/// Converts an S-expression tree into a ternlang `Expr`.
pub fn sexp_to_expr(sexp: &Sexp) -> Result<Expr, String> {
    match sexp {
        Sexp::Atom(s) => atom_to_expr(s),
        Sexp::List(items) => list_to_expr(items),
    }
}

fn atom_to_expr(s: &str) -> Result<Expr, String> {
    match s {
        "1"  | "+1" => Ok(Expr::TritLiteral(1)),
        "0"         => Ok(Expr::TritLiteral(0)),
        "-1"        => Ok(Expr::TritLiteral(-1)),
        "true"      => Ok(Expr::TritLiteral(1)),
        "false"     => Ok(Expr::TritLiteral(-1)),
        _           => Ok(Expr::Ident(s.to_string())),
    }
}

fn make_call(callee: &str, args: Vec<Expr>) -> Expr {
    Expr::Call { callee: callee.to_string(), args }
}

fn list_to_expr(items: &[Sexp]) -> Result<Expr, String> {
    if items.is_empty() {
        return Err("Empty S-expression list".to_string());
    }

    let head = match &items[0] {
        Sexp::Atom(s) => s.as_str(),
        _ => return Err("Expected operator/function name as first element".to_string()),
    };

    let args: Result<Vec<Expr>, _> = items[1..].iter().map(sexp_to_expr).collect();
    let args = args?;

    match head {
        // Arithmetic / ternary ops
        "+" | "add"  => {
            require(head, &args, 2)?;
            Ok(make_call("consensus", args))
        }
        "neg" | "-"  => {
            if args.len() == 1 {
                Ok(make_call("invert", args))
            } else if args.len() == 2 {
                let neg_b = make_call("invert", vec![args[1].clone()]);
                Ok(make_call("consensus", vec![args[0].clone(), neg_b]))
            } else {
                Err(format!("{}: expected 1 or 2 args, got {}", head, args.len()))
            }
        }
        "mul" | "*"  => { require(head, &args, 2)?; Ok(make_call("mul", args)) }
        "cons"       => { require(head, &args, 2)?; Ok(make_call("consensus", args)) }
        "invert"     => { require(head, &args, 1)?; Ok(make_call("invert", args)) }

        // Builtins
        "truth"    => Ok(make_call("truth",    vec![])),
        "hold"     => Ok(make_call("hold",     vec![])),
        "conflict" => Ok(make_call("conflict", vec![])),

        // If (3-way): (if cond on+1 on0 on-1)
        "if" => {
            require(head, &args, 4)?;
            // Map to ternlang IfTernary statement wrapped in a block expression.
            // Since Expr doesn't have an if-expr variant, we build a Call to a
            // synthetic helper that the codegen handles via ternary select.
            // For now: represent as (consensus (cond × branch+1), (invert cond × branch-1))
            // A cleaner approach: return a Match-like call. We use a Stmt::IfTernary
            // indirectly by returning the positive branch expression only (simplified).
            // Full if-expr requires extending Expr — leave as function call placeholder.
            Ok(make_call("__owlet_if__", args))
        }

        // Generic function call: (f arg1 arg2 ...)
        name => Ok(make_call(name, args)),
    }
}

fn require(head: &str, args: &[Expr], n: usize) -> Result<(), String> {
    if args.len() != n {
        Err(format!("{}: expected {} args, got {}", head, n, args.len()))
    } else {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Full Owlet program → ternlang Program
// ─────────────────────────────────────────────────────────────────────────────

/// Parse an Owlet program (multi-line S-expressions) into a ternlang `Program`.
///
/// Top-level `(fn name (params...) body)` become `Function` definitions.
/// Everything else becomes the body of a generated `main` function.
pub struct OwletParser;

impl OwletParser {
    /// Parse a complete Owlet source string into a ternlang `Program`.
    pub fn parse(source: &str) -> Result<Program, String> {
        let mut functions: Vec<Function> = Vec::new();
        let mut main_body: Vec<Stmt>     = Vec::new();

        // Accumulate balanced-paren expressions across lines
        let mut depth  = 0usize;
        let mut buffer = String::new();

        for line in source.lines() {
            // Strip comments from line
            let line = if let Some(idx) = line.find(';') { &line[..idx] } else { line }.trim();
            if line.is_empty() { continue; }

            for ch in line.chars() {
                match ch { '(' => depth += 1, ')' => depth = depth.saturating_sub(1), _ => {} }
            }
            buffer.push(' ');
            buffer.push_str(line);

            if depth == 0 && !buffer.trim().is_empty() {
                let sexp = parse_sexp(buffer.trim())?;
                buffer.clear();

                match &sexp {
                    Sexp::List(items) if !items.is_empty() => {
                        if let Sexp::Atom(head) = &items[0] {
                            match head.as_str() {
                                "fn" | "def" => {
                                    functions.push(parse_fn_def(items)?);
                                    continue;
                                }
                                "let" if items.len() >= 3 => {
                                    let name = match &items[1] {
                                        Sexp::Atom(n) => n.clone(),
                                        _ => return Err("let: expected name".to_string()),
                                    };
                                    let val = sexp_to_expr(&items[2])?;
                                    main_body.push(Stmt::Let { name, ty: Type::Trit, value: val });
                                    if items.len() >= 4 {
                                        let body = sexp_to_expr(&items[3])?;
                                        main_body.push(Stmt::Return(body));
                                    }
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }

                let expr = sexp_to_expr(&sexp)?;
                main_body.push(Stmt::Return(expr));
            }
        }

        if !main_body.is_empty() {
            functions.push(Function {
                name:        "main".to_string(),
                params:      vec![],
                return_type: Type::Trit,
                body:        main_body,
            });
        }

        Ok(Program { structs: vec![], agents: vec![], functions })
    }
}

fn parse_fn_def(items: &[Sexp]) -> Result<Function, String> {
    // (fn name (params...) body)
    if items.len() < 4 {
        return Err("fn: expected (fn name (params) body)".to_string());
    }
    let name = match &items[1] {
        Sexp::Atom(n) => n.clone(),
        _ => return Err("fn: expected function name".to_string()),
    };
    let params: Vec<(String, Type)> = match &items[2] {
        Sexp::List(ps) => ps.iter().map(|p| match p {
            Sexp::Atom(n) => Ok((n.clone(), Type::Trit)),
            _ => Err("fn: expected param name atom".to_string()),
        }).collect::<Result<_, _>>()?,
        Sexp::Atom(p) if p == "()" => vec![],
        _ => return Err("fn: expected param list".to_string()),
    };
    let body_expr = sexp_to_expr(&items[3])?;
    Ok(Function {
        name,
        params,
        return_type: Type::Trit,
        body: vec![Stmt::Return(body_expr)],
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_atom_pos() {
        let s = parse_sexp("1").unwrap();
        assert_eq!(sexp_to_expr(&s).unwrap(), Expr::TritLiteral(1));
    }

    #[test]
    fn test_parse_atom_neg() {
        let s = parse_sexp("-1").unwrap();
        assert_eq!(sexp_to_expr(&s).unwrap(), Expr::TritLiteral(-1));
    }

    #[test]
    fn test_parse_atom_zero() {
        let s = parse_sexp("0").unwrap();
        assert_eq!(sexp_to_expr(&s).unwrap(), Expr::TritLiteral(0));
    }

    #[test]
    fn test_parse_invert_call() {
        let s = parse_sexp("(neg -1)").unwrap();
        let expr = sexp_to_expr(&s).unwrap();
        assert!(matches!(expr, Expr::Call { ref callee, .. } if callee == "invert"));
    }

    #[test]
    fn test_parse_consensus_call() {
        let s = parse_sexp("(cons 1 0)").unwrap();
        let expr = sexp_to_expr(&s).unwrap();
        assert!(matches!(expr, Expr::Call { ref callee, .. } if callee == "consensus"));
    }

    #[test]
    fn test_parse_add_maps_to_consensus() {
        let s = parse_sexp("(+ 1 -1)").unwrap();
        let expr = sexp_to_expr(&s).unwrap();
        assert!(matches!(expr, Expr::Call { ref callee, .. } if callee == "consensus"));
    }

    #[test]
    fn test_parse_nested() {
        let s = parse_sexp("(neg (neg 1))").unwrap();
        let expr = sexp_to_expr(&s).unwrap();
        assert!(matches!(expr, Expr::Call { ref callee, .. } if callee == "invert"));
    }

    #[test]
    fn test_unmatched_paren_error() {
        assert!(parse_sexp("(+ 1 2").is_err());
    }

    #[test]
    fn test_empty_list_error() {
        let s = parse_sexp("()").unwrap();
        assert!(sexp_to_expr(&s).is_err());
    }

    #[test]
    fn test_owlet_fn_parse() {
        let src = "(fn negate (x) (neg x))";
        let prog = OwletParser::parse(src).unwrap();
        assert_eq!(prog.functions.len(), 1);
        assert_eq!(prog.functions[0].name, "negate");
        assert_eq!(prog.functions[0].params.len(), 1);
        assert_eq!(prog.functions[0].params[0].0, "x");
    }

    #[test]
    fn test_owlet_top_level_expr_becomes_main() {
        let src = "(neg -1)";
        let prog = OwletParser::parse(src).unwrap();
        assert_eq!(prog.functions.last().unwrap().name, "main");
    }

    #[test]
    fn test_owlet_comment_stripped() {
        let src = "; this is a comment\n(neg 1)";
        let prog = OwletParser::parse(src).unwrap();
        assert!(!prog.functions.is_empty());
    }

    #[test]
    fn test_owlet_let_binding() {
        let src = "(let x 1)";
        let prog = OwletParser::parse(src).unwrap();
        let main = prog.functions.last().unwrap();
        assert!(main.body.iter().any(|s| matches!(s, Stmt::Let { name, .. } if name == "x")));
    }

    #[test]
    fn test_owlet_mul() {
        let s = parse_sexp("(mul 1 -1)").unwrap();
        let expr = sexp_to_expr(&s).unwrap();
        assert!(matches!(expr, Expr::Call { ref callee, .. } if callee == "mul"));
    }
}
