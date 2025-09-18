#[derive(Debug, Clone)]
pub enum Type {
    Simple(String),
}

#[derive(Debug, Clone)]
pub struct Ident(pub String);
