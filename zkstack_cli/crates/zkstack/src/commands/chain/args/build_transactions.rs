use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{config::global_config, forge::ForgeScriptArgs, Prompt};

use crate::{
    consts::DEFAULT_UNSIGNED_TRANSACTIONS_DIR,
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_RPC_URL_PROMPT},
};

const CHAIN_SUBDIR: &str = "chain";

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct BuildTransactionsArgs {
    /// Output directory for the generated files.
    #[arg(long, short)]
    pub out: Option<PathBuf>,
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
}

impl BuildTransactionsArgs {
    pub fn fill_values_with_prompt(self, default_chain: String) -> BuildTransactionsArgsFinal {
        let chain_name = global_config().chain_name.clone();

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_RPC_URL_PROMPT)
                .default(LOCAL_RPC_URL)
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });

        BuildTransactionsArgsFinal {
            out: self
                .out
                .unwrap_or(PathBuf::from(DEFAULT_UNSIGNED_TRANSACTIONS_DIR).join(CHAIN_SUBDIR))
                .join(chain_name.unwrap_or(default_chain)),
            forge_args: self.forge_args,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BuildTransactionsArgsFinal {
    pub out: PathBuf,
    pub forge_args: ForgeScriptArgs,
    pub l1_rpc_url: String,
}
