use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::utils::docker::select_tag;

#[derive(Clone, Debug, Default, Serialize, Deserialize, ValueEnum)]
pub enum ExecutionMode {
    #[default]
    Release,
    Debug,
    Docker,
}

impl From<ExecutionMode> for common::server::ExecutionMode {
    fn from(mode: ExecutionMode) -> Self {
        match mode {
            ExecutionMode::Debug => Self::Debug,
            ExecutionMode::Release => Self::Release,
            ExecutionMode::Docker => Self::Docker,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct GenesisServerArgs {
    #[arg(long, default_value = "release")]
    pub mode: ExecutionMode,
    #[arg(long)]
    pub tag: Option<String>,
}

impl GenesisServerArgs {
    pub async fn fill_values_with_prompt(self) -> GenesisServerArgsFinal {
        let tag = if let ExecutionMode::Docker = self.mode {
            self.tag
                .or(select_tag().await.ok().or(Some("latest".to_string())))
        } else {
            None
        };

        GenesisServerArgsFinal {
            mode: self.mode,
            tag,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct GenesisServerArgsFinal {
    pub mode: ExecutionMode,
    pub tag: Option<String>,
}
