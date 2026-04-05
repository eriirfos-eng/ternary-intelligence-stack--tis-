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

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedToken(tok) =>
                write!(f, "[PARSE-001] Unexpected token '{tok}' — the lexer hit something it didn't expect. Check your syntax."),
            Self::ExpectedToken(expected, found) =>
                write!(f, "[PARSE-002] Expected {expected} but found '{found}'. The grammar demands it."),
            Self::InvalidTrit(val) =>
                write!(f, "[PARSE-003] '{val}' is not a valid trit. Trits are -1, 0, or +1. No in-betweens."),
            Self::NonExhaustiveMatch(msg) =>
                write!(f, "[PARSE-004] Non-exhaustive match: {msg}. Ternary has three states — cover all three or the compiler won't let you through."),
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { lex: Token::lexer(input) }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut structs = Vec::new();
        let mut agents = Vec::new();
        let mut functions = Vec::new();
        while self.peek_token().is_ok() {
            match self.peek_token()? {
                Token::Struct => structs.push(self.parse_struct_def()?),
                Token::Agent  => agents.push(self.parse_agent_def()?),
                _             => functions.push(self.parse_function()?),
            }
        }
        Ok(Program { structs, agents, functions })
    }

    fn parse_agent_def(&mut self) -> Result<AgentDef, ParseError> {
        self.expect(Token::Agent)?;
        let name = match self.next_token()? {
            Token::Ident(n) => n,
            t => return Err(ParseError::ExpectedToken("agent name".into(), format!("{:?}", t))),
        };
        self.expect(Token::LBrace)?;
        let mut methods = Vec::new();
        while self.peek_token()? != Token::RBrace {
            methods.push(self.parse_function()?);
        }
        self.expect(Token::RBrace)?;
        Ok(AgentDef { name, methods })
    }

    fn parse_struct_def(&mut self) -> Result<StructDef, ParseError> {
        self.expect(Token::Struct)?;
        let name = match self.next_token()? {
            Token::Ident(n) => n,
            t => return Err(ParseError::ExpectedToken("struct name".into(), format!("{:?}", t))),
        };
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();
        while self.peek_token()? != Token::RBrace {
            let field_name = match self.next_token()? {
                Token::Ident(n) => n,
                t => return Err(ParseError::ExpectedToken("field name".into(), format!("{:?}", t))),
            };
            self.expect(Token::Colon)?;
            let field_type = self.parse_type()?;
            fields.push((field_name, field_type));
            if let Ok(Token::Comma) = self.peek_token() { self.next_token()?; }
        }
        self.expect(Token::RBrace)?;
        Ok(StructDef { name, fields })
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
        let mut lhs = self.parse_unary_expr()?;
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

    /// Parse unary prefix expressions, then wrap with postfix (field access, `?`).
    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;
        loop {
            if let Ok(Token::Dot) = self.peek_token() {
                // Postfix: `.field` chain
                self.next_token()?; // consume `.`
                let field = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("field name".into(), format!("{:?}", t))),
                };
                expr = Expr::FieldAccess { object: Box::new(expr), field };
            } else if let Ok(Token::UncertainBranch) = self.peek_token() {
                // Postfix `?` — ternary error propagation.
                // Disambiguate from `if cond ? { }`: if the token after `?` is `{`, this
                // `?` belongs to the enclosing if/while statement — don't consume it.
                let mut lookahead = self.lex.clone();
                lookahead.next(); // skip `?`
                let after_q = lookahead.next();
                let is_uncertain_branch = matches!(after_q, Some(Ok(Token::LBrace)));
                if !is_uncertain_branch {
                    self.next_token()?; // consume `?`
                    expr = Expr::Propagate { expr: Box::new(expr) };
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.next_token()?;
        match token {
            // spawn AgentName           — local agent
            // spawn remote "addr" Name  — remote agent (Phase 5.1)
            Token::Spawn => {
                let node_addr = if let Ok(Token::Remote) = self.peek_token() {
                    self.next_token()?; // consume `remote`
                    match self.next_token()? {
                        Token::StringLit(addr) => Some(addr),
                        t => return Err(ParseError::ExpectedToken("node address string".into(), format!("{:?}", t))),
                    }
                } else {
                    None
                };
                let agent_name = match self.next_token()? {
                    Token::Ident(n) => n,
                    t => return Err(ParseError::ExpectedToken("agent name".into(), format!("{:?}", t))),
                };
                Ok(Expr::Spawn { agent_name, node_addr })
            }
            // await <agentref_expr> — receive from agent mailbox
            Token::Await => {
                let target = self.parse_unary_expr()?;
                Ok(Expr::Await { target: Box::new(target) })
            }
            Token::NodeId => Ok(Expr::NodeId),
            Token::Minus => {
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnaryOp { op: UnOp::Neg, expr: Box::new(expr) })
            }
            Token::TritLiteral => {
                let slice = self.lex.slice();
                let val = slice.parse::<i8>()
                    .map_err(|_| ParseError::InvalidTrit(slice.to_string()))?;
                Ok(Expr::TritLiteral(val))
            }
            // Semantic trit keywords: affirm=+1, tend=0, reject=-1
            Token::Affirm => Ok(Expr::TritLiteral(1)),
            Token::Tend   => Ok(Expr::TritLiteral(0)),
            Token::Reject => Ok(Expr::TritLiteral(-1)),
            Token::Int(val) => Ok(Expr::IntLiteral(val)),
            Token::StringLit(s) => Ok(Expr::StringLiteral(s)),
            Token::Ident(name) => {
                // cast(expr) built-in: returns Cast node
                if name == "cast" {
                    if let Ok(Token::LParen) = self.peek_token() {
                        self.next_token()?;
                        let inner = self.parse_expr()?;
                        self.expect(Token::RParen)?;
                        // Type is resolved by context (the let binding ty)
                        // Emit with placeholder Trit — semantic pass refines this
                        return Ok(Expr::Cast { expr: Box::new(inner), ty: Type::Trit });
                    }
                }

                if let Ok(Token::LParen) = self.peek_token() {
                    // Function call
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
                        Token::Ident(n)   => n,
                        Token::TritType   => "trit".to_string(),
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

            // send <target_expr> <message_expr>;
            Token::Send => {
                self.next_token()?;
                let target = self.parse_expr()?;
                let message = self.parse_expr()?;
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Send { target, message })
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
                // Could be: expr; OR field assignment: ident.field = value;
                let expr = self.parse_expr()?;

                // Check for field assignment: expr was `ident.field`, next is `=`
                if let Ok(Token::Assign) = self.peek_token() {
                    if let Expr::FieldAccess { object, field } = expr {
                        if let Expr::Ident(obj_name) = *object {
                            self.next_token()?; // consume `=`
                            let value = self.parse_expr()?;
                            self.expect(Token::Semicolon)?;
                            return Ok(Stmt::FieldSet { object: obj_name, field, value });
                        }
                    }
                    return Err(ParseError::UnexpectedToken("invalid assignment target".into()));
                }

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
            Token::AgentRef   => Ok(Type::AgentRef),
            Token::TritTensor => {
                self.expect(Token::LAngle)?;
                let mut dims = Vec::new();
                loop {
                    let d = match self.next_token()? {
                        Token::Int(v) => v as usize,
                        // TritLiteral matches "0" and "1" — accept them as dims
                        Token::TritLiteral => {
                            let s = self.lex.slice();
                            s.parse::<i8>().unwrap_or(0).max(0) as usize
                        }
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
                // Named struct type
                _        => Ok(Type::Named(name.clone())),
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

    #[test]
    fn test_parse_struct_def() {
        let input = "struct Signal { value: trit, weight: trit }";
        let mut parser = Parser::new(input);
        let s = parser.parse_struct_def().unwrap();
        assert_eq!(s.name, "Signal");
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0], ("value".to_string(), Type::Trit));
        assert_eq!(s.fields[1], ("weight".to_string(), Type::Trit));
    }

    #[test]
    fn test_parse_field_access() {
        let input = "let v: trit = sig.value;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Let { value: Expr::FieldAccess { field, .. }, .. } = stmt {
            assert_eq!(field, "value");
        } else {
            panic!("Expected FieldAccess in let binding");
        }
    }

    #[test]
    fn test_parse_field_set() {
        let input = "sig.value = 1;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        assert!(matches!(stmt, Stmt::FieldSet { .. }));
    }

    #[test]
    fn test_parse_cast() {
        let input = "let t: trit = cast(flag);";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Let { value: Expr::Cast { .. }, .. } = stmt {
            // ok
        } else {
            panic!("Expected Cast in let binding");
        }
    }

    #[test]
    fn test_parse_named_type() {
        let input = "let s: Signal;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Let { ty: Type::Named(name), .. } = stmt {
            assert_eq!(name, "Signal");
        } else {
            panic!("Expected Named type");
        }
    }

    #[test]
    fn test_parse_agent_def() {
        let input = r#"
            agent Voter {
                fn handle(msg: trit) -> trit {
                    match msg {
                         1 => { return  1; }
                         0 => { return  0; }
                        -1 => { return -1; }
                    }
                }
            }
        "#;
        let mut parser = Parser::new(input);
        let agent = parser.parse_agent_def().unwrap();
        assert_eq!(agent.name, "Voter");
        assert_eq!(agent.methods.len(), 1);
        assert_eq!(agent.methods[0].name, "handle");
    }

    #[test]
    fn test_parse_spawn() {
        let input = "let v: agentref = spawn Voter;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Let { ty: Type::AgentRef, value: Expr::Spawn { agent_name, node_addr }, .. } = stmt {
            assert_eq!(agent_name, "Voter");
            assert_eq!(node_addr, None);
        } else {
            panic!("Expected spawn in let binding");
        }
    }

    #[test]
    fn test_parse_spawn_remote() {
        let input = r#"let v: agentref = spawn remote "10.0.0.1:7373" Voter;"#;
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Let { ty: Type::AgentRef, value: Expr::Spawn { agent_name, node_addr }, .. } = stmt {
            assert_eq!(agent_name, "Voter");
            assert_eq!(node_addr, Some("10.0.0.1:7373".to_string()));
        } else {
            panic!("Expected remote spawn in let binding");
        }
    }

    #[test]
    fn test_parse_send() {
        let input = "send v 1;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        assert!(matches!(stmt, Stmt::Send { .. }));
    }

    #[test]
    fn test_parse_await() {
        let input = "let reply: trit = await v;";
        let mut parser = Parser::new(input);
        let stmt = parser.parse_stmt().unwrap();
        if let Stmt::Let { value: Expr::Await { .. }, .. } = stmt {
            // ok
        } else {
            panic!("Expected await in let binding");
        }
    }
}
