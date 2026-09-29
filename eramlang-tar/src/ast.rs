//! Abstract Syntax Tree produced by the parser.

#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Int,
    Float,
    Bool,
    Str,
    Char,
    Void,
    Array(Box<TypeAnnotation>),
    Named(String), // struct or enum name
    Unknown,       // inferred later
}

impl std::fmt::Display for TypeAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeAnnotation::Int => write!(f, "int"),
            TypeAnnotation::Float => write!(f, "float"),
            TypeAnnotation::Bool => write!(f, "bool"),
            TypeAnnotation::Str => write!(f, "string"),
            TypeAnnotation::Char => write!(f, "char"),
            TypeAnnotation::Void => write!(f, "void"),
            TypeAnnotation::Array(t) => write!(f, "[{}]", t),
            TypeAnnotation::Named(n) => write!(f, "{}", n),
            TypeAnnotation::Unknown => write!(f, "?"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Function(FunctionDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Statement(Statement),
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeAnnotation,
    pub body: Block,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeAnnotation,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, TypeAnnotation)>,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
    pub line: usize,
    pub col: usize,
}

pub type Block = Vec<Statement>;

#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        name: String,
        mutable: bool,
        ty: TypeAnnotation,
        value: Expr,
        line: usize,
        col: usize,
    },
    Assign {
        target: Expr,
        value: Expr,
        line: usize,
        col: usize,
    },
    CompoundAssign {
        target: Expr,
        op: BinOp,
        value: Expr,
        line: usize,
        col: usize,
    },
    ExprStmt(Expr),
    Return {
        value: Option<Expr>,
        line: usize,
        col: usize,
    },
    If {
        cond: Expr,
        then_block: Block,
        else_block: Option<Block>,
        line: usize,
        col: usize,
    },
    While {
        cond: Expr,
        body: Block,
        line: usize,
        col: usize,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        body: Block,
        line: usize,
        col: usize,
    },
    Break {
        line: usize,
        col: usize,
    },
    Continue {
        line: usize,
        col: usize,
    },
    Block(Block),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Integer(i64, usize, usize),
    Float(f64, usize, usize),
    Bool(bool, usize, usize),
    Str(String, usize, usize),
    Char(char, usize, usize),
    Identifier(String, usize, usize),
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
        line: usize,
        col: usize,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        line: usize,
        col: usize,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
        line: usize,
        col: usize,
    },
    ArrayLiteral {
        elements: Vec<Expr>,
        line: usize,
        col: usize,
    },
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
        line: usize,
        col: usize,
    },
    StructLiteral {
        name: String,
        fields: Vec<(String, Expr)>,
        line: usize,
        col: usize,
    },
    MemberAccess {
        object: Box<Expr>,
        field: String,
        line: usize,
        col: usize,
    },
    EnumValue {
        enum_name: Option<String>,
        variant: String,
        line: usize,
        col: usize,
    },
    Match {
        subject: Box<Expr>,
        arms: Vec<(String, MatchArmBody)>,
        line: usize,
        col: usize,
    },
}

#[derive(Debug, Clone)]
pub enum MatchArmBody {
    Expr(Box<Expr>),
    Block(Block),
}

impl Expr {
    pub fn pos(&self) -> (usize, usize) {
        match self {
            Expr::Integer(_, l, c)
            | Expr::Float(_, l, c)
            | Expr::Bool(_, l, c)
            | Expr::Str(_, l, c)
            | Expr::Char(_, l, c)
            | Expr::Identifier(_, l, c)
            | Expr::Binary {
                line: l, col: c, ..
            }
            | Expr::Unary {
                line: l, col: c, ..
            }
            | Expr::Call {
                line: l, col: c, ..
            }
            | Expr::ArrayLiteral {
                line: l, col: c, ..
            }
            | Expr::Index {
                line: l, col: c, ..
            }
            | Expr::StructLiteral {
                line: l, col: c, ..
            }
            | Expr::MemberAccess {
                line: l, col: c, ..
            }
            | Expr::EnumValue {
                line: l, col: c, ..
            }
            | Expr::Match {
                line: l, col: c, ..
            } => (*l, *c),
        }
    }
}
