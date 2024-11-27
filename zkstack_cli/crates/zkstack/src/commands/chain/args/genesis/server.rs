use clap::Parser;
use common::server::ExecutionMode;
use serde::{Deserialize, Serialize};

use crate::{commands::args::run::Mode, utils::docker::select_tag};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct GenesisServerArgs {
    #[arg(long, default_value = "release")]
    pub mode: Mode,
    #[arg(long)]
    pub tag: Option<String>,
}

impl GenesisServerArgs {
    pub async fn fill_values_with_prompt(self) -> GenesisServerArgsFinal {
        let tag = if let Mode::Docker = self.mode {
            self.tag
                .or(select_tag().await.ok().or(Some("latest".to_string())))
        } else {
            None
        };

        GenesisServerArgsFinal {
            mode: self.mode.as_execution_mode(tag),
        }
    }
}

#[derive(Debug)]
pub struct GenesisServerArgsFinal {
    pub mode: ExecutionMode,
}
