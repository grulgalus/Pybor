#[derive(Debug, Clone)]
pub enum Type {
    Int,
    String,
    RefMut(Box<Type>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(String),
    Number(i32),
    BinaryOp(Box<Expr>, String, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Statement {
    Assignment { name: String, value: Expr },
    Return(Expr),
    FunctionDef {
        name: String,
        params: Vec<(String, Type)>,
        body: Vec<Statement>,
    },
}
