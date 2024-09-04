use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR, MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT,
        MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT,
    },
};

const DEFAULT_OUT_DIR: &str = "transactions";

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemBuildArgs {
    /// Deploy ecosystem contracts
    #[arg(long)]
    pub build_ecosystem: bool,
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    /// Address of the transaction sender.
    #[arg(long)]
    pub sender: String,
    /// Output directory for the generated files.
    #[arg(long, short)]
    pub out: Option<String>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
}

impl EcosystemBuildArgs {
    pub fn fill_values_with_prompt(self) -> EcosystemBuildArgsFinal {
        let ecosystem_contracts_path = match &self.ecosystem_contracts_path {
            Some(path) => Some(path.clone()),
            None => {
                let input_path: String = Prompt::new(MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT)
                    .allow_empty()
                    .validate_with(|val: &String| {
                        if val.is_empty() {
                            return Ok(());
                        }
                        PathBuf::from_str(val)
                            .map(|_| ())
                            .map_err(|_| MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR.to_string())
                    })
                    .ask();
                if input_path.is_empty() {
                    None
                } else {
                    Some(input_path.into())
                }
            }
        };

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_L1_RPC_URL_PROMPT)
                .default(LOCAL_RPC_URL)
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });
        EcosystemBuildArgsFinal {
            sender: self.sender,
            out: self.out.unwrap_or_else(|| DEFAULT_OUT_DIR.into()),
            forge_args: self.forge_args.clone(),
            build_ecosystem: self.build_ecosystem,
            ecosystem_contracts_path,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcosystemBuildArgsFinal {
    pub sender: String,
    pub out: String,
    pub forge_args: ForgeScriptArgs,
    pub build_ecosystem: bool,
    pub ecosystem_contracts_path: Option<PathBuf>,
    pub l1_rpc_url: String,
}
