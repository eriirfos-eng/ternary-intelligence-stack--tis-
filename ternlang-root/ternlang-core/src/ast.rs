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
    /// field access: `object.field`
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    /// cast(expr) — type coercion built-in
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    /// spawn AgentName — creates an agent instance, evaluates to AgentRef
    Spawn {
        agent_name: String,
    },
    /// await <agentref_expr> — receive result from agent mailbox
    Await {
        target: Box<Expr>,
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
    /// send <agentref_expr> <message_expr>;
    Send {
        target: Expr,
        message: Expr,
    },
    /// instance.field = value;
    FieldSet {
        object: String,
        field: String,
        value: Expr,
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
    /// User-defined struct type
    Named(String),
    /// Handle to a running agent instance
    AgentRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Stmt>,
}

/// Top-level struct definition: `struct Name { field: type, ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

/// Top-level agent definition: `agent Name { fn handle(msg: trit) -> trit { ... } }`
/// v0.1: agents have a single `handle` method that processes each incoming message.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentDef {
    pub name: String,
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub structs: Vec<StructDef>,
    pub agents: Vec<AgentDef>,
    pub functions: Vec<Function>,
}
