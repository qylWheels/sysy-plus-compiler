use super::item::*;

#[derive(Debug, Clone)]
pub struct CompileUnit {
    pub(crate) items: Vec<Item>,
}
