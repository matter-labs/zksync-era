use std::path::PathBuf;

use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT},
};

const DEFAULT_OUT_DIR: &str = "transactions";

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemTransactionArgs {
    /// Address of the transaction sender.
    pub sender: String,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    /// Output directory for the generated files.
    #[arg(long, short)]
    pub out: Option<PathBuf>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
}

impl EcosystemTransactionArgs {
    pub fn fill_values_with_prompt(self) -> EcosystemTransactionArgsFinal {
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
        EcosystemTransactionArgsFinal {
            sender: self.sender,
            out: self.out.unwrap_or(DEFAULT_OUT_DIR.into()),
            forge_args: self.forge_args.clone(),
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcosystemTransactionArgsFinal {
    pub sender: String,
    pub out: PathBuf,
    pub forge_args: ForgeScriptArgs,
    pub l1_rpc_url: String,
}
