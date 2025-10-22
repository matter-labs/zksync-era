use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Paths to source files used by contract related commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFiles {
    /// Path to the directory with contracts, that represents era-contracts repo
    pub contracts_path: PathBuf,
    /// Path to the directory with default configs inside the repository.
    /// It's the configs, that used as templates for new chains, in era they are placed `etc/env/file_based`
    pub default_configs_path: PathBuf,
}
