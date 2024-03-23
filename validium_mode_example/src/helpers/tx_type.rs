use std::fmt::Display;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TxType {
    Commit,
    Prove,
    Execute,
}

impl Display for TxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxType::Commit => write!(f, "Commit"),
            TxType::Prove => write!(f, "Prove"),
            TxType::Execute => write!(f, "Execute"),
        }
    }
}
