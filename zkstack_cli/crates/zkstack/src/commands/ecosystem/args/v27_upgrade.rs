use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use url::Url;
use zkstack_cli_common::{forge::ForgeScriptArgs, Prompt};
use zkstack_cli_types::L1Network;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT},
};

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum V27UpgradeStage {
    // Deploy contracts + init everything the governance will need to approve the upgrade
    NoGovernancePrepare,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct V27UpgradeArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, value_enum)]
    ecosystem_upgrade_stage: V27UpgradeStage,
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
}

impl V27UpgradeArgs {
    pub fn fill_values_with_prompt(self, l1_network: L1Network, dev: bool) -> V27UpgradeArgsFinal {
        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_L1_RPC_URL_PROMPT);
            if dev {
                return LOCAL_RPC_URL.to_string();
            }
            if l1_network == L1Network::Localhost {
                prompt = prompt.default(LOCAL_RPC_URL);
            }
            prompt
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });
        V27UpgradeArgsFinal {
            forge_args: self.forge_args,
            ecosystem_upgrade_stage: self.ecosystem_upgrade_stage,
            ecosystem_contracts_path: self.ecosystem_contracts_path,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct V27UpgradeArgsFinal {
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, value_enum)]
    pub ecosystem_upgrade_stage: V27UpgradeStage,
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: String,
}
