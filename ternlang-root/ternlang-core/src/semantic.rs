use crate::ast::*;

#[derive(Debug)]
pub enum SemanticError {
    TypeMismatch { expected: Type, found: Type },
    UndefinedVariable(String),
}

pub struct SemanticAnalyzer {
    // Basic symbol table for now
    scopes: Vec<std::collections::HashMap<String, Type>>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            scopes: vec![std::collections::HashMap::new()],
        }
    }

    pub fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), SemanticError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let val_ty = self.infer_expr_type(value)?;
                if val_ty != *ty {
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
            Stmt::Decorated { stmt, .. } => {
                self.check_stmt(stmt)
            }
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
                // bind loop variable as Trit (tensors yield trits)
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
            Stmt::Use { .. } => Ok(()), // module resolution is a future pass
        }
    }

    fn infer_expr_type(&self, expr: &Expr) -> Result<Type, SemanticError> {
        match expr {
            Expr::TritLiteral(_) => Ok(Type::Trit),
            Expr::IntLiteral(_) => Ok(Type::Int),
            Expr::Ident(name) => {
                for scope in self.scopes.iter().rev() {
                    if let Some(ty) = scope.get(name) {
                        return Ok(ty.clone());
                    }
                }
                Err(SemanticError::UndefinedVariable(name.clone()))
            }
            Expr::BinaryOp { op, lhs, rhs } => {
                let l_ty = self.infer_expr_type(lhs)?;
                let r_ty = self.infer_expr_type(rhs)?;
                if l_ty != r_ty {
                    return Err(SemanticError::TypeMismatch { expected: l_ty, found: r_ty });
                }
                // For simplicity, assume same type for output
                Ok(l_ty)
            }
            Expr::UnaryOp { expr, .. } => self.infer_expr_type(expr),
            Expr::Call { .. } => Ok(Type::Trit), // Mocked for now
        }
    }
}
