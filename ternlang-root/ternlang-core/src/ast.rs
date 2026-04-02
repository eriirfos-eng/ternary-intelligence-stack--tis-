#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    TritLiteral(i8),
    IntLiteral(i64),
    Ident(String),
    BinaryOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    UnaryOp {
        op: UnOp,
        expr: Box<Expr>,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Equal,
    NotEqual,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        ty: Type,
        value: Expr,
    },
    IfTernary {
        condition: Expr,
        on_pos: Box<Stmt>,   // branch when +1
        on_zero: Box<Stmt>,  // branch when  0
        on_neg: Box<Stmt>,   // branch when -1
    },
    Match {
        condition: Expr,
        arms: Vec<(i8, Stmt)>,
    },
    /// for <var> in <iter_expr> { body }
    ForIn {
        var: String,
        iter: Expr,
        body: Box<Stmt>,
    },
    /// while <condition> ? { on_pos } else { on_zero } else { on_neg }
    WhileTernary {
        condition: Expr,
        on_pos: Box<Stmt>,
        on_zero: Box<Stmt>,
        on_neg: Box<Stmt>,
    },
    /// loop { body } — infinite loop, exited via break
    Loop {
        body: Box<Stmt>,
    },
    Break,
    Continue,
    Block(Vec<Stmt>),
    Return(Expr),
    Expr(Expr),
    Decorated {
        directive: String,
        stmt: Box<Stmt>,
    },
    /// use path::to::module;
    Use {
        path: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Trit,
    TritTensor { dims: Vec<usize> },
    Int,
    Bool,
    Float,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}
