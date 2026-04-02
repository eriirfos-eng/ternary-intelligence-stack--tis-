use crate::lexer::Token;
use crate::ast::*;
use logos::{Logos, Lexer};

pub struct Parser<'a> {
    lex: Lexer<'a, Token>,
}

#[derive(Debug)]
pub enum ParseError {
    UnexpectedToken(String),
    ExpectedToken(String, String),
    InvalidTrit(String),
    NonExhaustiveMatch(String),
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { lex: Token::lexer(input) }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut functions = Vec::new();
        while self.peek_token().is_ok() {
            functions.push(self.parse_function()?);
        }
        Ok(Program { functions })
    }

    pub fn parse_function(&mut self) -> Result<Function, ParseError> {
        self.expect(Token::Fn)?;
        let name = match self.next_token()? {
            Token::Ident(n) => n,
            t => return Err(ParseError::ExpectedToken("function name".into(), format!("{:?}", t))),
        };

        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        if self.peek_token()? != Token::RParen {
            loop {
                let p_name = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("parameter name".into(), format!("{:?}", t))),
                };
                self.expect(Token::Colon)?;
                let p_type = self.parse_type()?;
                params.push((p_name, p_type));
                if self.peek_token()? == Token::Comma { self.next_token()?; } else { break; }
            }
        }
        self.expect(Token::RParen)?;
        self.expect(Token::Arrow)?;
        let return_type = self.parse_type()?;
        let body = match self.parse_block()? {
            Stmt::Block(stmts) => stmts,
            _ => unreachable!(),
        };
        Ok(Function { name, params, return_type, body })
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        self.lex.next()
            .map(|res| res.map_err(|_| ParseError::UnexpectedToken("Invalid token".into())))
            .transpose()?
            .ok_or(ParseError::UnexpectedToken("EOF".into()))
    }

    fn peek_token(&mut self) -> Result<Token, ParseError> {
        let mut cloned = self.lex.clone();
        cloned.next()
            .map(|res| res.map_err(|_| ParseError::UnexpectedToken("Invalid token".into())))
            .transpose()?
            .ok_or(ParseError::UnexpectedToken("EOF".into()))
    }

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_binary_expr(0)
    }

    fn parse_binary_expr(&mut self, min_prec: i8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_primary_expr()?;
        loop {
            let Ok(op_token) = self.peek_token() else { break };
            let prec = self.get_precedence(&op_token);
            if prec < min_prec { break; }
            self.next_token()?;
            let rhs = self.parse_binary_expr(prec + 1)?;
            lhs = Expr::BinaryOp {
                op: self.token_to_binop(op_token),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn get_precedence(&self, token: &Token) -> i8 {
        match token {
            Token::Or                      => 0,
            Token::And                     => 1,
            Token::Equal | Token::NotEqual => 2,
            Token::Plus  | Token::Minus    => 3,
            Token::Star                    => 4,
            _ => -1,
        }
    }

    fn token_to_binop(&self, token: Token) -> BinOp {
        match token {
            Token::Plus     => BinOp::Add,
            Token::Minus    => BinOp::Sub,
            Token::Star     => BinOp::Mul,
            Token::Equal    => BinOp::Equal,
            Token::NotEqual => BinOp::NotEqual,
            Token::And      => BinOp::And,
            Token::Or       => BinOp::Or,
            _ => unreachable!(),
        }
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.next_token()?;
        match token {
            Token::Minus => {
                let expr = self.parse_primary_expr()?;
                Ok(Expr::UnaryOp { op: UnOp::Neg, expr: Box::new(expr) })
            }
            Token::TritLiteral => {
                let slice = self.lex.slice();
                let val = slice.parse::<i8>()
                    .map_err(|_| ParseError::InvalidTrit(slice.to_string()))?;
                Ok(Expr::TritLiteral(val))
            }
            Token::Int(val) => Ok(Expr::IntLiteral(val)),
            Token::Ident(name) => {
                if let Ok(Token::LParen) = self.peek_token() {
                    self.next_token()?;
                    let mut args = Vec::new();
                    if self.peek_token()? != Token::RParen {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.peek_token()? == Token::Comma {
                                self.next_token()?;
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(Token::RParen)?;
                    Ok(Expr::Call { callee: name, args })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            _ => Err(ParseError::UnexpectedToken(format!("{:?}", token))),
        }
    }

    pub fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let token = self.peek_token()?;
        match token {
            Token::At => {
                self.next_token()?;
                let dir = match self.next_token()? {
                    Token::SparseSkip  => "sparseskip".to_string(),
                    Token::Ident(n)    => n,
                    t => return Err(ParseError::ExpectedToken("directive".into(), format!("{:?}", t))),
                };
                let stmt = self.parse_stmt()?;
                Ok(Stmt::Decorated { directive: dir, stmt: Box::new(stmt) })
            }

            Token::Use => {
                self.next_token()?;
                let mut path = Vec::new();
                loop {
                    // Accept both identifiers and reserved type keywords as path segments
                    let segment = match self.next_token()? {
                        Token::Ident(n)  => n,
                        Token::TritType  => "trit".to_string(),
                        Token::TritTensor => "trittensor".to_string(),
                        t => return Err(ParseError::ExpectedToken("module path segment".into(), format!("{:?}", t))),
                    };
                    path.push(segment);
                    if let Ok(Token::DoubleColon) = self.peek_token() {
                        self.next_token()?;
                    } else {
                        break;
                    }
                }
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Use { path })
            }

            Token::Let => {
                self.next_token()?;
                // optional mut
                let _mutable = if let Ok(Token::Mut) = self.peek_token() {
                    self.next_token()?; true
                } else { false };

                let name = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("identifier".into(), format!("{:?}", t))),
                };
                self.expect(Token::Colon)?;
                let ty = self.parse_type()?;
                let value = if let Ok(Token::Assign) = self.peek_token() {
                    self.next_token()?;
                    self.parse_expr()?
                } else {
                    Expr::TritLiteral(0)
                };
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Let { name, ty, value })
            }

            Token::If => {
                self.next_token()?;
                let condition = self.parse_expr()?;
                self.expect(Token::UncertainBranch)?;
                let on_pos  = Box::new(self.parse_block()?);
                self.expect(Token::Else)?;
                let on_zero = Box::new(self.parse_block()?);
                self.expect(Token::Else)?;
                let on_neg  = Box::new(self.parse_block()?);
                Ok(Stmt::IfTernary { condition, on_pos, on_zero, on_neg })
            }

            Token::Match => {
                self.next_token()?;
                let condition = self.parse_expr()?;
                self.expect(Token::LBrace)?;
                let mut arms = Vec::new();
                while self.peek_token()? != Token::RBrace {
                    let val = match self.next_token()? {
                        Token::TritLiteral => {
                            let slice = self.lex.slice();
                            slice.parse::<i8>().map_err(|_| ParseError::InvalidTrit(slice.to_string()))?
                        }
                        t => return Err(ParseError::ExpectedToken("trit literal".into(), format!("{:?}", t))),
                    };
                    self.expect(Token::FatArrow)?;
                    let stmt = self.parse_stmt()?;
                    arms.push((val, stmt));
                }
                self.expect(Token::RBrace)?;

                // Enforce exhaustiveness: must have arms for -1, 0, and +1
                let has_pos  = arms.iter().any(|(v, _)| *v ==  1);
                let has_zero = arms.iter().any(|(v, _)| *v ==  0);
                let has_neg  = arms.iter().any(|(v, _)| *v == -1);
                if !has_pos || !has_zero || !has_neg {
                    let missing: Vec<&str> = [
                        if !has_pos  { Some("1 (truth)")    } else { None },
                        if !has_zero { Some("0 (hold)")     } else { None },
                        if !has_neg  { Some("-1 (conflict)") } else { None },
                    ].iter().filter_map(|x| *x).collect();
                    return Err(ParseError::NonExhaustiveMatch(
                        format!("match missing arms: {}", missing.join(", "))
                    ));
                }

                Ok(Stmt::Match { condition, arms })
            }

            // for <var> in <expr> { body }
            Token::For => {
                self.next_token()?;
                let var = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("loop variable".into(), format!("{:?}", t))),
                };
                self.expect(Token::In)?;
                let iter = self.parse_expr()?;
                let body = Box::new(self.parse_block()?);
                Ok(Stmt::ForIn { var, iter, body })
            }

            // while <condition> ? { on_pos } else { on_zero } else { on_neg }
            Token::While => {
                self.next_token()?;
                let condition = self.parse_expr()?;
                self.expect(Token::UncertainBranch)?;
                let on_pos  = Box::new(self.parse_block()?);
                self.expect(Token::Else)?;
                let on_zero = Box::new(self.parse_block()?);
                self.expect(Token::Else)?;
                let on_neg  = Box::new(self.parse_block()?);
                Ok(Stmt::WhileTernary { condition, on_pos, on_zero, on_neg })
            }

            // loop { body }
            Token::Loop => {
                self.next_token()?;
                let body = Box::new(self.parse_block()?);
                Ok(Stmt::Loop { body })
            }

            Token::Break => {
                self.next_token()?;
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Break)
            }

            Token::Continue => {
                self.next_token()?;
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Continue)
            }

            Token::Return => {
                self.next_token()?;
                let expr = self.parse_expr()?;
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Return(expr))
            }

            Token::LBrace => self.parse_block(),

            _ => {
                let expr = self.parse_expr()?;
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_block(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::LBrace)?;
        let mut stmts = Vec::new();
        while self.peek_token()? != Token::RBrace {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(Token::RBrace)?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let token = self.next_token()?;
        match token {
            Token::TritType   => Ok(Type::Trit),
            Token::TritTensor => {
                self.expect(Token::LAngle)?;
                let mut dims = Vec::new();
                loop {
                    let d = match self.next_token()? {
                        Token::Int(v) => v as usize,
                        t => return Err(ParseError::ExpectedToken("dimension".into(), format!("{:?}", t))),
                    };
                    dims.push(d);
                    if self.peek_token()? == Token::Ident("x".to_string()) {
                        self.next_token()?;
                    } else {
                        break;
                    }
                }
                self.expect(Token::RAngle)?;
                Ok(Type::TritTensor { dims })
            }
            Token::Ident(ref name) => match name.as_str() {
                "int"    => Ok(Type::Int),
                "float"  => Ok(Type::Float),
                "bool"   => Ok(Type::Bool),
                "string" => Ok(Type::String),
                _ => Err(ParseError::UnexpectedToken(format!("unknown type: {}", name))),
            },
            _ => Err(ParseError::UnexpectedToken(format!("{:?}", token))),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        let token = self.next_token()?;
        if token == expected {
            Ok(())
        } else {
            Err(ParseError::ExpectedToken(format!("{:?}", expected), format!("{:?}", token)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function() {
        let input = "fn invert(signal: trit) -> trit { return -signal; }";
        let mut parser = Parser::new(input);
        let func = parser.parse_function().unwrap();
        assert_eq!(func.name, "invert");
        assert_eq!(func.params[0].1, Type::Trit);
        assert_eq!(func.return_type, Type::Trit);
    }

    #[test]
    fn test_parse_match() {
        let input = "match x { 1 => return 1; 0 => return 0; -1 => return -1; }";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Match { arms, .. } = stmt {
            assert_eq!(arms.len(), 3);
            assert_eq!(arms[0].0, 1);
            assert_eq!(arms[1].0, 0);
            assert_eq!(arms[2].0, -1);
        } else {
            panic!("Expected Match");
        }
    }

    #[test]
    fn test_match_exhaustiveness_enforced() {
        // Missing hold (0) arm — must fail
        let input = "match x { 1 => return 1; -1 => return -1; }";
        let mut parser = Parser::new(input);
        let result = parser.parse_stmt();
        assert!(matches!(result, Err(ParseError::NonExhaustiveMatch(_))),
            "expected NonExhaustiveMatch error");
    }

    #[test]
    fn test_parse_for_loop() {
        let input = "for item in weights { return item; }";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        assert!(matches!(stmt, Stmt::ForIn { .. }));
    }

    #[test]
    fn test_parse_loop_break() {
        let input = "loop { break; }";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        assert!(matches!(stmt, Stmt::Loop { .. }));
    }

    #[test]
    fn test_parse_use() {
        let input = "use std::trit;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Use { path } = stmt {
            assert_eq!(path, vec!["std", "trit"]);
        } else {
            panic!("Expected Use");
        }
    }

    #[test]
    fn test_parse_mut_let() {
        let input = "let mut signal: trit = 1;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        assert!(matches!(stmt, Stmt::Let { .. }));
    }
}
