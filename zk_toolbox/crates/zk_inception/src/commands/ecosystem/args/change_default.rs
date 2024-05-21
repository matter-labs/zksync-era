use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct ChangeDefaultHyperchain {
    pub name: Option<String>,
}
