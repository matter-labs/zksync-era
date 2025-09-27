use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFiles {
    /// Path to contracts source files
    pub contracts_path: PathBuf,
    /// Path to default configs
    pub default_configs_path: PathBuf,
}
