use crate::ast::*;

#[derive(Debug)]
pub enum SemanticError {
    TypeMismatch { expected: Type, found: Type },
    UndefinedVariable(String),
    UndefinedStruct(String),
    UndefinedField { struct_name: String, field: String },
}

pub struct SemanticAnalyzer {
    scopes: Vec<std::collections::HashMap<String, Type>>,
    /// Struct definitions registered at program level
    struct_defs: std::collections::HashMap<String, Vec<(String, Type)>>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            scopes: vec![std::collections::HashMap::new()],
            struct_defs: std::collections::HashMap::new(),
        }
    }

    /// Register all struct definitions before checking functions.
    pub fn register_structs(&mut self, structs: &[StructDef]) {
        for s in structs {
            self.struct_defs.insert(s.name.clone(), s.fields.clone());
        }
    }

    pub fn check_program(&mut self, program: &Program) -> Result<(), SemanticError> {
        self.register_structs(&program.structs);
        // Register agent method signatures so calls inside agents are resolved.
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
        self.scopes.push(std::collections::HashMap::new());
        // Bind parameters into the function scope
        for (name, ty) in &func.params {
            self.scopes.last_mut().unwrap().insert(name.clone(), ty.clone());
        }
        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }
        self.scopes.pop();
        Ok(())
    }

    pub fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), SemanticError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let val_ty = self.infer_expr_type(value)?;
                // For Named (struct) types: accept zero-initializer (TritLiteral 0 = hold)
                // For Cast: accept any source type — cast is always valid
                let type_ok = val_ty == *ty
                    || matches!(value, Expr::Cast { .. })
                    || (matches!(ty, Type::Named(_)) && val_ty == Type::Trit);
                if !type_ok {
                    return Err(SemanticError::TypeMismatch { expected: ty.clone(), found: val_ty });
                }
                self.scopes.last_mut().unwrap().insert(name.clone(), ty.clone());
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
                for (_val, stmt) in arms {
                    self.check_stmt(stmt)?;
                }
                Ok(())
            }
            Stmt::Block(stmts) => {
                self.scopes.push(std::collections::HashMap::new());
                for s in stmts {
                    self.check_stmt(s)?;
                }
                self.scopes.pop();
                Ok(())
            }
            Stmt::Decorated { stmt, .. } => self.check_stmt(stmt),
            Stmt::Return(expr) => {
                self.infer_expr_type(expr)?;
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr_type(expr)?;
                Ok(())
            }
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
            Stmt::Loop { body } => self.check_stmt(body),
            Stmt::Break | Stmt::Continue => Ok(()),
            Stmt::Use { .. } => Ok(()),
            Stmt::Send { target, message } => {
                self.infer_expr_type(target)?;
                self.infer_expr_type(message)?;
                Ok(())
            }
            Stmt::FieldSet { object, field, value } => {
                // Look up object type in scope, verify field exists, check value type
                let obj_ty = self.lookup_var(object)?;
                if let Type::Named(struct_name) = obj_ty {
                    let field_ty = self.lookup_field(&struct_name, field)?;
                    let val_ty = self.infer_expr_type(value)?;
                    if val_ty != field_ty {
                        return Err(SemanticError::TypeMismatch { expected: field_ty, found: val_ty });
                    }
                    Ok(())
                } else {
                    // Non-struct field set — tolerate for trit fields in simple programs
                    self.infer_expr_type(value)?;
                    Ok(())
                }
            }
        }
    }

    fn lookup_var(&self, name: &str) -> Result<Type, SemanticError> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Ok(ty.clone());
            }
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

    fn infer_expr_type(&self, expr: &Expr) -> Result<Type, SemanticError> {
        match expr {
            Expr::TritLiteral(_) => Ok(Type::Trit),
            Expr::IntLiteral(_)  => Ok(Type::Int),
            Expr::Ident(name)    => self.lookup_var(name),
            Expr::BinaryOp { op: _, lhs, rhs } => {
                let l_ty = self.infer_expr_type(lhs)?;
                let r_ty = self.infer_expr_type(rhs)?;
                if l_ty != r_ty {
                    return Err(SemanticError::TypeMismatch { expected: l_ty, found: r_ty });
                }
                Ok(l_ty)
            }
            Expr::UnaryOp { expr, .. } => self.infer_expr_type(expr),
            Expr::Call { .. }          => Ok(Type::Trit), // mocked — full resolution is Phase 4 todo
            Expr::Cast { ty, .. }      => Ok(ty.clone()),
            Expr::Spawn { .. }         => Ok(Type::AgentRef),
            Expr::Await { .. }         => Ok(Type::Trit),  // agents communicate in trits v0.1
            Expr::FieldAccess { object, field } => {
                let obj_ty = self.infer_expr_type(object)?;
                if let Type::Named(struct_name) = obj_ty {
                    self.lookup_field(&struct_name, field)
                } else {
                    // Accessing a field on a non-struct — return Trit for builtins
                    Ok(Type::Trit)
                }
            }
        }
    }
}
