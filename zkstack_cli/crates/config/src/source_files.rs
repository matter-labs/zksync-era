use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Paths to source files used by contract related commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFiles {
    /// Path to contracts source files
    pub contracts_path: PathBuf,
    /// Path to default configs
    pub default_configs_path: PathBuf,
}
