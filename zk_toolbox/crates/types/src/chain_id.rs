use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChainId(pub u32);

impl Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for ChainId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}
