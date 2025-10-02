use std::path::PathBuf;

use clap::Parser;
use url::Url;
use zkstack_cli_common::Prompt;

use crate::commands::dev::{
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_INVALID_L1_RPC_URL_ERR, MSG_PROMPT_L1_RPC_URL, MSG_PROMPT_SECRET_KEY,
        MSG_PROMPT_TRANSACTION_FILE,
    },
};

const DEFAULT_TRANSACTION_CONFIRMATIONS: usize = 5;

#[derive(Debug, Parser)]
pub struct SendTransactionsArgs {
    #[clap(long)]
    pub file: Option<PathBuf>,
    #[clap(long)]
    pub private_key: Option<String>,
    #[clap(long)]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub confirmations: Option<usize>,
}

#[derive(Debug)]
pub struct SendTransactionsArgsFinal {
    pub file: PathBuf,
    pub private_key: String,
    pub l1_rpc_url: String,
    pub confirmations: usize,
}

impl SendTransactionsArgs {
    pub fn fill_values_with_prompt(self) -> SendTransactionsArgsFinal {
        let file = self
            .file
            .unwrap_or_else(|| Prompt::new(MSG_PROMPT_TRANSACTION_FILE).ask());

        let private_key = self
            .private_key
            .unwrap_or_else(|| Prompt::new(MSG_PROMPT_SECRET_KEY).ask());

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            Prompt::new(MSG_PROMPT_L1_RPC_URL)
                .default(LOCAL_RPC_URL)
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_INVALID_L1_RPC_URL_ERR.to_string())
                })
                .ask()
        });

        let confirmations = self
            .confirmations
            .unwrap_or(DEFAULT_TRANSACTION_CONFIRMATIONS);

        SendTransactionsArgsFinal {
            file,
            private_key,
            l1_rpc_url,
            confirmations,
        }
    }
}
