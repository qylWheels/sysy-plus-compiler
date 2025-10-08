use std::fmt;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Type {
    Simple(String),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Ident(pub String);

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
