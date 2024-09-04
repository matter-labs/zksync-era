use std::path::PathBuf;

use clap::Parser;
use common::{forge::ForgeScriptArgs, PromptConfirm};
use serde::{Deserialize, Serialize};

use crate::{
    commands::chain::args::genesis::GenesisArgs,
    messages::{MSG_DEPLOY_ECOSYSTEM_PROMPT, MSG_GENESIS_ARGS_HELP},
};

const DEFAULT_OUT_DIR: &str = "transactions";

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemArgs {
    /// Deploy ecosystem contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub build_ecosystem: Option<bool>,
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
}

impl EcosystemArgs {
    pub fn fill_values_with_prompt(self) -> EcosystemArgsFinal {
        let build_ecosystem = self.build_ecosystem.unwrap_or_else(|| {
            PromptConfirm::new(MSG_DEPLOY_ECOSYSTEM_PROMPT)
                .default(true)
                .ask()
        });

        EcosystemArgsFinal {
            build_ecosystem,
            ecosystem_contracts_path: self.ecosystem_contracts_path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemArgsFinal {
    pub build_ecosystem: bool,
    pub ecosystem_contracts_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemBuildArgs {
    /// Address of the transaction sender.
    #[arg(long)]
    pub sender: String,
    /// Output directory for the generated files.
    #[arg(long, short)]
    pub out: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(flatten, next_help_heading = MSG_GENESIS_ARGS_HELP)]
    #[serde(flatten)]
    pub genesis_args: GenesisArgs,
}

impl EcosystemBuildArgs {
    pub fn fill_values_with_prompt(self) -> EcosystemBuildArgsFinal {
        let ecosystem = self.ecosystem.fill_values_with_prompt();

        EcosystemBuildArgsFinal {
            sender: self.sender,
            out: self.out.unwrap_or_else(|| DEFAULT_OUT_DIR.into()),
            ecosystem,
            forge_args: self.forge_args.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcosystemBuildArgsFinal {
    pub sender: String,
    pub out: String,
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
}
