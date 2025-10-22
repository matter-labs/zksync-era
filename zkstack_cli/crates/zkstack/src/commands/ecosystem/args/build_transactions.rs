use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{forge::ForgeScriptArgs, Prompt};
use zksync_basic_types::H160;

use crate::{
    commands::ecosystem::args::common::CommonEcosystemArgs,
    consts::DEFAULT_UNSIGNED_TRANSACTIONS_DIR,
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_BRIDGEHUB, MSG_L1_RPC_URL_INVALID_ERR, MSG_RPC_URL_PROMPT, MSG_SENDER_ADDRESS_PROMPT,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct BuildTransactionsArgs {
    #[command(flatten)]
    pub common: CommonEcosystemArgs,

    /// Address of the transaction sender.
    #[clap(long)]
    pub sender: Option<String>,
    /// Output directory for the generated files.
    #[arg(long, short)]
    pub out: Option<PathBuf>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub bridgehub: String,
}

impl BuildTransactionsArgs {
    pub fn fill_values_with_prompt(self) -> Result<BuildTransactionsFinal> {
        let sender = self.sender.unwrap_or_else(|| {
            Prompt::new(MSG_SENDER_ADDRESS_PROMPT)
                .validate_with(|val: &String| -> Result<(), String> {
                    H160::from_str(val).map_or_else(|err| Err(err.to_string()), |_| Ok(()))
                })
                .ask()
        });

        let l1_rpc_url = self.common.l1_rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_RPC_URL_PROMPT)
                .default(LOCAL_RPC_URL)
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });

        // Parse bridgehub address
        let bridgehub_address = self
            .bridgehub
            .parse::<H160>()
            .with_context(|| format!("Invalid bridgehub address format: {}", self.bridgehub))?;

        Ok(BuildTransactionsFinal {
            sender,
            out: self.out.unwrap_or(DEFAULT_UNSIGNED_TRANSACTIONS_DIR.into()),
            forge_args: self.forge_args.clone(),
            l1_rpc_url,
            bridgehub_address,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildTransactionsFinal {
    pub sender: String,
    pub out: PathBuf,
    pub forge_args: ForgeScriptArgs,
    pub l1_rpc_url: String,
    pub bridgehub_address: H160,
}
