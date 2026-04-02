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
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            lex: Token::lexer(input),
        }
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
            t => return Err(ParseError::ExpectedToken("function name".to_string(), format!("{:?}", t))),
        };

        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        if self.peek_token()? != Token::RParen {
            loop {
                let p_name = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("parameter name".to_string(), format!("{:?}", t))),
                };
                self.expect(Token::Colon)?;
                let p_type = self.parse_type()?;
                params.push((p_name, p_type));

                if self.peek_token()? == Token::Comma {
                    self.next_token()?;
                } else {
                    break;
                }
            }
        }
        self.expect(Token::RParen)?;

        self.expect(Token::Arrow)?;
        let return_type = self.parse_type()?;

        let body = match self.parse_block()? {
            Stmt::Block(stmts) => stmts,
            _ => unreachable!(),
        };

        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        self.lex.next().map(|res| res.map_err(|_| ParseError::UnexpectedToken("Invalid token".to_string()))).transpose()?.ok_or(ParseError::UnexpectedToken("EOF".to_string()))
    }

    fn peek_token(&mut self) -> Result<Token, ParseError> {
        let mut cloned = self.lex.clone();
        cloned.next().map(|res| res.map_err(|_| ParseError::UnexpectedToken("Invalid token".to_string()))).transpose()?.ok_or(ParseError::UnexpectedToken("EOF".to_string()))
    }

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_binary_expr(0)
    }

    fn parse_binary_expr(&mut self, min_prec: i8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_primary_expr()?;

        loop {
            let op_res = self.peek_token();
            if op_res.is_err() { break; }
            let op_token = op_res.unwrap();
            let prec = self.get_precedence(&op_token);
            if prec < min_prec { break; }

            self.next_token()?; // consume op
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
            Token::Plus | Token::Minus => 1,
            Token::Star => 2,
            Token::Equal => 0,
            _ => -1,
        }
    }

    fn token_to_binop(&self, token: Token) -> BinOp {
        match token {
            Token::Plus => BinOp::Add,
            Token::Minus => BinOp::Sub,
            Token::Star => BinOp::Mul,
            Token::Equal => BinOp::Equal,
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
                let val = slice.parse::<i8>().map_err(|_| ParseError::InvalidTrit(slice.to_string()))?;
                Ok(Expr::TritLiteral(val))
            }
            Token::Int(val) => Ok(Expr::IntLiteral(val)),
            Token::Ident(name) => {
                if let Ok(Token::LParen) = self.peek_token() {
                    self.next_token()?; // consume '('
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
                let next = self.next_token()?;
                if next != Token::RParen {
                    return Err(ParseError::ExpectedToken(")".to_string(), format!("{:?}", next)));
                }
                Ok(expr)
            }
            _ => Err(ParseError::UnexpectedToken(format!("{:?}", token))),
        }
    }

    pub fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let token = self.peek_token()?;
        match token {
            Token::At => {
                self.next_token()?; // consume '@'
                let dir = match self.next_token()? {
                    Token::SparseSkip => "sparseskip".to_string(),
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("directive".to_string(), format!("{:?}", t))),
                };
                let stmt = self.parse_stmt()?;
                Ok(Stmt::Decorated { directive: dir, stmt: Box::new(stmt) })
            }
            Token::Let => {
                self.next_token()?; // consume 'let'
                let name = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("identifier".to_string(), format!("{:?}", t))),
                };
                self.expect(Token::Colon)?;
                let ty = self.parse_type()?;
                
                let value = if let Ok(Token::Assign) = self.peek_token() {
                    self.next_token()?; // consume '='
                    self.parse_expr()?
                } else {
                    // Default value
                    Expr::TritLiteral(0)
                };
                
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Let { name, ty, value })
            }
            Token::If => {
                self.next_token()?; // consume 'if'
                let condition = self.parse_expr()?;
                self.expect(Token::UncertainBranch)?; // '?' for ternary if
                let on_pos = Box::new(self.parse_block()?);
                self.expect(Token::Else)?;
                let on_zero = Box::new(self.parse_block()?);
                self.expect(Token::Else)?;
                let on_neg = Box::new(self.parse_block()?);
                Ok(Stmt::IfTernary { condition, on_pos, on_zero, on_neg })
            }
            Token::Match => {
                self.next_token()?; // consume 'match'
                let condition = self.parse_expr()?;
                self.expect(Token::LBrace)?;
                let mut arms = Vec::new();
                while self.peek_token()? != Token::RBrace {
                    let val = match self.next_token()? {
                        Token::TritLiteral => {
                            let slice = self.lex.slice();
                            slice.parse::<i8>().map_err(|_| ParseError::InvalidTrit(slice.to_string()))?
                        }
                        t => return Err(ParseError::ExpectedToken("trit literal".to_string(), format!("{:?}", t))),
                    };
                    self.expect(Token::FatArrow)?;
                    let stmt = self.parse_stmt()?;
                    arms.push((val, stmt));
                }
                self.expect(Token::RBrace)?;
                Ok(Stmt::Match { condition, arms })
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
            Token::TritType => Ok(Type::Trit),
            Token::TritTensor => {
                self.expect(Token::LAngle)?;
                let mut dims = Vec::new();
                loop {
                    let d = match self.next_token()? {
                        Token::Int(v) => v as usize,
                        t => return Err(ParseError::ExpectedToken("dimension".to_string(), format!("{:?}", t))),
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
            Token::Ident(name) if name == "int" => Ok(Type::Int),
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
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].0, "signal");
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
}
