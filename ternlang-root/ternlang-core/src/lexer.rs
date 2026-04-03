use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\f]+")] // Skip whitespace
#[logos(skip(r"//[^\n]*", allow_greedy = true))]   // Skip line comments
pub enum Token {
    // Ternary Specific
    #[token("-1")]
    #[token("0")]
    #[token("1")]
    TritLiteral,

    #[token("?")]
    UncertainBranch,

    #[token("trit", priority = 3)]
    TritType,

    #[token("trittensor", priority = 3)]
    TritTensor,

    #[token("sparseskip", priority = 3)]
    SparseSkip,


    // Standard Keywords
    #[token("if", priority = 3)]
    If,

    #[token("else", priority = 3)]
    Else,

    #[token("fn", priority = 3)]
    Fn,

    #[token("return", priority = 3)]
    Return,

    #[token("let", priority = 3)]
    Let,

    #[token("match", priority = 3)]
    Match,

    #[token("for", priority = 3)]
    For,

    #[token("in", priority = 3)]
    In,

    #[token("while", priority = 3)]
    While,

    #[token("loop", priority = 3)]
    Loop,

    #[token("break", priority = 3)]
    Break,

    #[token("continue", priority = 3)]
    Continue,

    #[token("mut", priority = 3)]
    Mut,

    #[token("use", priority = 3)]
    Use,

    #[token("module", priority = 3)]
    Module,

    #[token("pub", priority = 3)]
    Pub,

    #[token("struct", priority = 3)]
    Struct,

    // Actor model keywords
    #[token("agent", priority = 3)]
    Agent,

    #[token("spawn", priority = 3)]
    Spawn,

    #[token("send", priority = 3)]
    Send,

    #[token("await", priority = 3)]
    Await,

    #[token("agentref", priority = 3)]
    AgentRef,

    #[token("remote", priority = 3)]
    Remote,

    #[token("nodeid", priority = 3)]
    NodeId,

    // Operators
    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Star,

    #[token("=")]
    Assign,

    #[token("==")]
    Equal,

    #[token("=>")]
    FatArrow,

    #[token("!=")]
    NotEqual,

    #[token("::")]
    DoubleColon,

    #[token("&&")]
    And,

    #[token("||")]
    Or,

    // Delimiters
    #[token("(", priority = 3)]
    LParen,

    #[token(")", priority = 3)]
    RParen,

    #[token("{", priority = 3)]
    LBrace,

    #[token("}", priority = 3)]
    RBrace,

    #[token("[", priority = 3)]
    LBracket,

    #[token("]", priority = 3)]
    RBracket,

    #[token("<", priority = 3)]
    LAngle,

    #[token(">", priority = 3)]
    RAngle,

    #[token(",", priority = 3)]
    Comma,

    #[token(";", priority = 3)]
    Semicolon,

    #[token(":")]
    Colon,

    #[token("@")]
    At,

    #[token("->")]
    Arrow,

    #[token(".")]
    Dot,

    // Literals
    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string(), priority = 1)]
    Ident(String),

    #[regex("[0-9]+", |lex| lex.slice().parse::<i64>().ok(), priority = 1)]
    Int(i64),

    /// Double-quoted string literal: "value"
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    }, priority = 2)]
    StringLit(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer() {
        let input = "let x: trit = 1; if x ? { return 0; }";
        let mut lex = Token::lexer(input);

        assert_eq!(lex.next(), Some(Ok(Token::Let)));
        assert_eq!(lex.next(), Some(Ok(Token::Ident("x".to_string()))));
        assert_eq!(lex.next(), Some(Ok(Token::Colon)));
        assert_eq!(lex.next(), Some(Ok(Token::TritType)));
        assert_eq!(lex.next(), Some(Ok(Token::Assign)));
        assert_eq!(lex.next(), Some(Ok(Token::TritLiteral)));
        assert_eq!(lex.next(), Some(Ok(Token::Semicolon)));
        assert_eq!(lex.next(), Some(Ok(Token::If)));
        assert_eq!(lex.next(), Some(Ok(Token::Ident("x".to_string()))));
        assert_eq!(lex.next(), Some(Ok(Token::UncertainBranch)));
        assert_eq!(lex.next(), Some(Ok(Token::LBrace)));
        assert_eq!(lex.next(), Some(Ok(Token::Return)));
        assert_eq!(lex.next(), Some(Ok(Token::TritLiteral)));
        assert_eq!(lex.next(), Some(Ok(Token::Semicolon)));
        assert_eq!(lex.next(), Some(Ok(Token::RBrace)));
    }
}
