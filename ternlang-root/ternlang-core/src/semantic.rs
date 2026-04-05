use crate::ast::*;

// ─── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SemanticError {
    TypeMismatch { expected: Type, found: Type },
    UndefinedVariable(String),
    UndefinedStruct(String),
    UndefinedField { struct_name: String, field: String },
    UndefinedFunction(String),
    ReturnTypeMismatch { function: String, expected: Type, found: Type },
    ArgCountMismatch { function: String, expected: usize, found: usize },
    ArgTypeMismatch { function: String, param_index: usize, expected: Type, found: Type },
    /// `?` used on an expression that doesn't return trit
    PropagateOnNonTrit { found: Type },
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { expected, found } =>
                write!(f, "[TYPE-001] Type mismatch: expected {expected:?}, found {found:?}. Binary types don't map cleanly to ternary space."),
            Self::UndefinedVariable(n) =>
                write!(f, "[SCOPE-001] '{n}' is undefined. Hold state — declare before use."),
            Self::UndefinedStruct(n) =>
                write!(f, "[STRUCT-001] Struct '{n}' doesn't exist. The type system can't find it."),
            Self::UndefinedField { struct_name, field } =>
                write!(f, "[STRUCT-002] Struct '{struct_name}' has no field '{field}'. Check your definition."),
            Self::UndefinedFunction(n) =>
                write!(f, "[FN-001] '{n}' is not defined. Did you forget to declare it or import its module?"),
            Self::ReturnTypeMismatch { function, expected, found } =>
                write!(f, "[FN-002] Function '{function}' declared return type {expected:?} but returned {found:?}. Ternary contracts are strict."),
            Self::ArgCountMismatch { function, expected, found } =>
                write!(f, "[FN-003] '{function}' expects {expected} arg(s), got {found}. Arity is not optional."),
            Self::ArgTypeMismatch { function, param_index, expected, found } =>
                write!(f, "[FN-004] '{function}' arg {param_index}: expected {expected:?}, found {found:?}. Types travel with their values."),
            Self::PropagateOnNonTrit { found } =>
                write!(f, "[PROP-001] '?' used on a {found:?} expression. Only trit-returning functions can signal conflict. The third state requires a trit."),
        }
    }
}

// ─── Full function signature ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FunctionSig {
    /// Parameter types in declaration order. None = variadic / unknown (built-ins with flexible arity).
    pub params: Option<Vec<Type>>,
    pub return_type: Type,
}

impl FunctionSig {
    fn exact(params: Vec<Type>, return_type: Type) -> Self {
        Self { params: Some(params), return_type }
    }
    fn variadic(return_type: Type) -> Self {
        Self { params: None, return_type }
    }
}

// ─── Analyzer ────────────────────────────────────────────────────────────────

pub struct SemanticAnalyzer {
    scopes:           Vec<std::collections::HashMap<String, Type>>,
    struct_defs:      std::collections::HashMap<String, Vec<(String, Type)>>,
    func_signatures:  std::collections::HashMap<String, FunctionSig>,
    /// Set while type-checking a function body so Return stmts can be validated.
    current_fn_name:       Option<String>,
    current_fn_return:     Option<Type>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut sigs: std::collections::HashMap<String, FunctionSig> = std::collections::HashMap::new();

        // ── std::trit built-ins ────────────────────────────────────────────
        sigs.insert("consensus".into(), FunctionSig::exact(vec![Type::Trit, Type::Trit], Type::Trit));
        sigs.insert("invert".into(),    FunctionSig::exact(vec![Type::Trit],             Type::Trit));
        sigs.insert("truth".into(),     FunctionSig::exact(vec![],                       Type::Trit));
        sigs.insert("hold".into(),      FunctionSig::exact(vec![],                       Type::Trit));
        sigs.insert("conflict".into(),  FunctionSig::exact(vec![],                       Type::Trit));
        sigs.insert("mul".into(),       FunctionSig::exact(vec![Type::Trit, Type::Trit], Type::Trit));

        // ── std::tensor ────────────────────────────────────────────────────
        sigs.insert("matmul".into(),   FunctionSig::variadic(Type::TritTensor { dims: vec![0, 0] }));
        sigs.insert("sparsity".into(), FunctionSig::variadic(Type::Int));
        sigs.insert("shape".into(),    FunctionSig::variadic(Type::Int));
        sigs.insert("zeros".into(),    FunctionSig::variadic(Type::TritTensor { dims: vec![0, 0] }));

        // ── std::io ────────────────────────────────────────────────────────
        sigs.insert("print".into(),    FunctionSig::variadic(Type::Trit));
        sigs.insert("println".into(),  FunctionSig::variadic(Type::Trit));

        // ── std::math ──────────────────────────────────────────────────────
        sigs.insert("abs".into(),      FunctionSig::exact(vec![Type::Int],  Type::Int));
        sigs.insert("min".into(),      FunctionSig::exact(vec![Type::Int, Type::Int], Type::Int));
        sigs.insert("max".into(),      FunctionSig::exact(vec![Type::Int, Type::Int], Type::Int));

        // ── ml::quantize ───────────────────────────────────────────────────
        sigs.insert("quantize".into(), FunctionSig::variadic(Type::TritTensor { dims: vec![0, 0] }));
        sigs.insert("threshold".into(),FunctionSig::variadic(Type::Float));

        // ── ml::inference ──────────────────────────────────────────────────
        sigs.insert("forward".into(),  FunctionSig::variadic(Type::TritTensor { dims: vec![0, 0] }));
        sigs.insert("argmax".into(),   FunctionSig::variadic(Type::Int));

        // ── type coercion ──────────────────────────────────────────────────
        sigs.insert("cast".into(),     FunctionSig::variadic(Type::Trit));

        Self {
            scopes: vec![std::collections::HashMap::new()],
            struct_defs: std::collections::HashMap::new(),
            func_signatures: sigs,
            current_fn_name: None,
            current_fn_return: None,
        }
    }

    // ── Registration ─────────────────────────────────────────────────────────

    pub fn register_structs(&mut self, structs: &[StructDef]) {
        for s in structs {
            self.struct_defs.insert(s.name.clone(), s.fields.clone());
        }
    }

    pub fn register_functions(&mut self, functions: &[Function]) {
        for f in functions {
            let params = f.params.iter().map(|(_, ty)| ty.clone()).collect();
            self.func_signatures.insert(
                f.name.clone(),
                FunctionSig::exact(params, f.return_type.clone()),
            );
        }
    }

    pub fn register_agents(&mut self, agents: &[AgentDef]) {
        for agent in agents {
            for method in &agent.methods {
                let params = method.params.iter().map(|(_, ty)| ty.clone()).collect();
                let sig = FunctionSig::exact(params, method.return_type.clone());
                self.func_signatures.insert(method.name.clone(), sig.clone());
                self.func_signatures.insert(
                    format!("{}::{}", agent.name, method.name),
                    sig,
                );
            }
        }
    }

    // ── Entry points ─────────────────────────────────────────────────────────

    pub fn check_program(&mut self, program: &Program) -> Result<(), SemanticError> {
        self.register_structs(&program.structs);
        self.register_functions(&program.functions);
        self.register_agents(&program.agents);
        for agent in &program.agents {
            for method in &agent.methods {
                self.check_function(method)?;
            }
        }
        for func in &program.functions {
            self.check_function(func)?;
        }
        Ok(())
    }

    fn check_function(&mut self, func: &Function) -> Result<(), SemanticError> {
        // Track return type context for this function body.
        let prev_name   = self.current_fn_name.take();
        let prev_return = self.current_fn_return.take();
        self.current_fn_name   = Some(func.name.clone());
        self.current_fn_return = Some(func.return_type.clone());

        self.scopes.push(std::collections::HashMap::new());
        for (name, ty) in &func.params {
            self.scopes.last_mut().unwrap().insert(name.clone(), ty.clone());
        }
        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }
        self.scopes.pop();

        // Restore outer context (handles nested definitions if ever needed).
        self.current_fn_name   = prev_name;
        self.current_fn_return = prev_return;
        Ok(())
    }

    // ── Statement checking ───────────────────────────────────────────────────

    pub fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), SemanticError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let val_ty = self.infer_expr_type(value)?;
                let type_ok = val_ty == *ty
                    || matches!(value, Expr::Cast { .. })
                    || (matches!(ty, Type::Named(_)) && val_ty == Type::Trit)
                    || (matches!(ty, Type::TritTensor { .. }) && matches!(val_ty, Type::TritTensor { .. }))
                    || (*ty == Type::AgentRef && val_ty == Type::AgentRef);
                if !type_ok {
                    return Err(SemanticError::TypeMismatch { expected: ty.clone(), found: val_ty });
                }
                self.scopes.last_mut().unwrap().insert(name.clone(), ty.clone());
                Ok(())
            }

            Stmt::Return(expr) => {
                let found = self.infer_expr_type(expr)?;
                if let (Some(fn_name), Some(expected)) = (&self.current_fn_name, &self.current_fn_return) {
                    // Allow TritTensor shape flexibility and AgentRef, cast
                    let ok = found == *expected
                        || matches!(expr, Expr::Cast { .. })
                        || (matches!(expected, Type::TritTensor { .. }) && matches!(found, Type::TritTensor { .. }))
                        || (matches!(expected, Type::Named(_)) && found == Type::Trit);
                    if !ok {
                        return Err(SemanticError::ReturnTypeMismatch {
                            function: fn_name.clone(),
                            expected: expected.clone(),
                            found,
                        });
                    }
                }
                Ok(())
            }

            Stmt::IfTernary { condition, on_pos, on_zero, on_neg } => {
                let cond_ty = self.infer_expr_type(condition)?;
                if cond_ty != Type::Trit {
                    return Err(SemanticError::TypeMismatch { expected: Type::Trit, found: cond_ty });
                }
                self.check_stmt(on_pos)?;
                self.check_stmt(on_zero)?;
                self.check_stmt(on_neg)?;
                Ok(())
            }

            Stmt::Match { condition, arms } => {
                let cond_ty = self.infer_expr_type(condition)?;
                if cond_ty != Type::Trit {
                    return Err(SemanticError::TypeMismatch { expected: Type::Trit, found: cond_ty });
                }
                for (_val, arm_stmt) in arms {
                    self.check_stmt(arm_stmt)?;
                }
                Ok(())
            }

            Stmt::Block(stmts) => {
                self.scopes.push(std::collections::HashMap::new());
                for s in stmts { self.check_stmt(s)?; }
                self.scopes.pop();
                Ok(())
            }

            Stmt::Decorated { stmt, .. } => self.check_stmt(stmt),

            Stmt::Expr(expr) => { self.infer_expr_type(expr)?; Ok(()) }

            Stmt::ForIn { var, iter, body } => {
                self.infer_expr_type(iter)?;
                self.scopes.push(std::collections::HashMap::new());
                self.scopes.last_mut().unwrap().insert(var.clone(), Type::Trit);
                self.check_stmt(body)?;
                self.scopes.pop();
                Ok(())
            }

            Stmt::WhileTernary { condition, on_pos, on_zero, on_neg } => {
                let cond_ty = self.infer_expr_type(condition)?;
                if cond_ty != Type::Trit {
                    return Err(SemanticError::TypeMismatch { expected: Type::Trit, found: cond_ty });
                }
                self.check_stmt(on_pos)?;
                self.check_stmt(on_zero)?;
                self.check_stmt(on_neg)?;
                Ok(())
            }

            Stmt::Loop { body }   => self.check_stmt(body),
            Stmt::Break           => Ok(()),
            Stmt::Continue        => Ok(()),
            Stmt::Use { .. }      => Ok(()),

            Stmt::Send { target, message } => {
                self.infer_expr_type(target)?;
                self.infer_expr_type(message)?;
                Ok(())
            }

            Stmt::FieldSet { object, field, value } => {
                let obj_ty = self.lookup_var(object)?;
                if let Type::Named(struct_name) = obj_ty {
                    let field_ty = self.lookup_field(&struct_name, field)?;
                    let val_ty   = self.infer_expr_type(value)?;
                    if val_ty != field_ty {
                        return Err(SemanticError::TypeMismatch { expected: field_ty, found: val_ty });
                    }
                } else {
                    self.infer_expr_type(value)?;
                }
                Ok(())
            }

            Stmt::IndexSet { object, row, col, value } => {
                self.lookup_var(object)?;
                self.infer_expr_type(row)?;
                self.infer_expr_type(col)?;
                self.infer_expr_type(value)?;
                Ok(())
            }
        }
    }

    // ── Expression type inference ─────────────────────────────────────────────

    fn infer_expr_type(&self, expr: &Expr) -> Result<Type, SemanticError> {
        match expr {
            Expr::TritLiteral(_)   => Ok(Type::Trit),
            Expr::IntLiteral(_)    => Ok(Type::Int),
            Expr::StringLiteral(_) => Ok(Type::String),
            Expr::Ident(name)      => self.lookup_var(name),

            Expr::BinaryOp { lhs, rhs, .. } => {
                let l = self.infer_expr_type(lhs)?;
                let r = self.infer_expr_type(rhs)?;
                if l != r {
                    return Err(SemanticError::TypeMismatch { expected: l, found: r });
                }
                Ok(l)
            }

            Expr::UnaryOp { expr, .. } => self.infer_expr_type(expr),

            Expr::Call { callee, args } => {
                let sig = self.func_signatures.get(callee.as_str())
                    .ok_or_else(|| SemanticError::UndefinedFunction(callee.clone()))?
                    .clone();

                // Argument arity + type checking (only for exact signatures).
                if let Some(param_types) = &sig.params {
                    if args.len() != param_types.len() {
                        return Err(SemanticError::ArgCountMismatch {
                            function: callee.clone(),
                            expected: param_types.len(),
                            found:    args.len(),
                        });
                    }
                    for (i, (arg, expected_ty)) in args.iter().zip(param_types.iter()).enumerate() {
                        let found_ty = self.infer_expr_type(arg)?;
                        // Allow TritTensor shape flexibility and cast coercion.
                        let ok = found_ty == *expected_ty
                            || matches!(arg, Expr::Cast { .. })
                            || (matches!(expected_ty, Type::TritTensor { .. })
                                && matches!(found_ty, Type::TritTensor { .. }))
                            || (matches!(expected_ty, Type::Named(_)) && found_ty == Type::Trit);
                        if !ok {
                            return Err(SemanticError::ArgTypeMismatch {
                                function:    callee.clone(),
                                param_index: i,
                                expected:    expected_ty.clone(),
                                found:       found_ty,
                            });
                        }
                    }
                } else {
                    // Variadic — still infer arg types to catch undefined variables.
                    for arg in args { self.infer_expr_type(arg)?; }
                }

                Ok(sig.return_type)
            }

            Expr::Cast { ty, .. }     => Ok(ty.clone()),
            Expr::Spawn { .. }        => Ok(Type::AgentRef),
            Expr::Await { .. }        => Ok(Type::Trit),
            Expr::NodeId              => Ok(Type::String),

            Expr::Propagate { expr } => {
                let inner = self.infer_expr_type(expr)?;
                if inner != Type::Trit {
                    return Err(SemanticError::PropagateOnNonTrit { found: inner });
                }
                Ok(Type::Trit)
            }

            Expr::FieldAccess { object, field } => {
                let obj_ty = self.infer_expr_type(object)?;
                if let Type::Named(struct_name) = obj_ty {
                    self.lookup_field(&struct_name, field)
                } else {
                    Ok(Type::Trit)
                }
            }

            Expr::Index { object, row, col } => {
                self.infer_expr_type(object)?;
                self.infer_expr_type(row)?;
                self.infer_expr_type(col)?;
                Ok(Type::Trit)
            }
        }
    }

    // ── Scope helpers ─────────────────────────────────────────────────────────

    fn lookup_var(&self, name: &str) -> Result<Type, SemanticError> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) { return Ok(ty.clone()); }
        }
        Err(SemanticError::UndefinedVariable(name.to_string()))
    }

    fn lookup_field(&self, struct_name: &str, field: &str) -> Result<Type, SemanticError> {
        let fields = self.struct_defs.get(struct_name)
            .ok_or_else(|| SemanticError::UndefinedStruct(struct_name.to_string()))?;
        fields.iter()
            .find(|(f, _)| f == field)
            .map(|(_, ty)| ty.clone())
            .ok_or_else(|| SemanticError::UndefinedField {
                struct_name: struct_name.to_string(),
                field: field.to_string(),
            })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn check(src: &str) -> Result<(), SemanticError> {
        let mut parser = Parser::new(src);
        let prog = parser.parse_program().expect("parse failed");
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.check_program(&prog)
    }

    fn check_ok(src: &str) {
        assert!(check(src).is_ok(), "expected ok, got: {:?}", check(src));
    }

    fn check_err(src: &str) {
        assert!(check(src).is_err(), "expected error but check passed");
    }

    // ── Return type validation ────────────────────────────────────────────────

    #[test]
    fn test_return_correct_type() {
        check_ok("fn f() -> trit { return 1; }");
    }

    #[test]
    fn test_return_wrong_type_caught() {
        // Returns Int but declared -> trit
        check_err("fn f() -> trit { let x: int = 42; return x; }");
    }

    #[test]
    fn test_return_trit_in_trit_fn() {
        check_ok("fn decide(a: trit, b: trit) -> trit { return consensus(a, b); }");
    }

    // ── Argument count checking ───────────────────────────────────────────────

    #[test]
    fn test_call_correct_arity() {
        check_ok("fn f() -> trit { return consensus(1, -1); }");
    }

    #[test]
    fn test_call_too_few_args_caught() {
        check_err("fn f() -> trit { return consensus(1); }");
    }

    #[test]
    fn test_call_too_many_args_caught() {
        check_err("fn f() -> trit { return invert(1, 1); }");
    }

    // ── Argument type checking ────────────────────────────────────────────────

    #[test]
    fn test_call_wrong_arg_type_caught() {
        // invert expects trit, passing int literal 42 directly — int is not trit
        check_err("fn f() -> trit { let x: int = 42; return invert(x); }");
    }

    #[test]
    fn test_call_correct_arg_type() {
        check_ok("fn f(a: trit) -> trit { return invert(a); }");
    }

    // ── Undefined function ────────────────────────────────────────────────────

    #[test]
    fn test_undefined_function_caught() {
        check_err("fn f() -> trit { return doesnt_exist(1); }");
    }

    // ── User-defined function forward references ──────────────────────────────

    #[test]
    fn test_user_fn_return_type_registered() {
        check_ok("fn helper(a: trit) -> trit { return invert(a); } fn main() -> trit { return helper(1); }");
    }

    #[test]
    fn test_user_fn_wrong_return_caught() {
        check_err("fn helper(a: trit) -> trit { let x: int = 1; return x; }");
    }

    // ── Undefined variable ────────────────────────────────────────────────────

    #[test]
    fn test_undefined_variable_caught() {
        check_err("fn f() -> trit { return ghost_var; }");
    }

    #[test]
    fn test_defined_variable_ok() {
        check_ok("fn f() -> trit { let x: trit = 1; return x; }");
    }

    // ── Struct field types ────────────────────────────────────────────────────

    #[test]
    fn test_struct_field_access_ok() {
        check_ok("struct S { val: trit } fn f(s: S) -> trit { return s.val; }");
    }
}
