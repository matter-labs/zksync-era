use std::path::PathBuf;

use clap::Parser;
use common::Prompt;
use url::Url;

use crate::defaults::LOCAL_RPC_URL;

#[derive(Debug, Parser)]
pub struct SendTransactionsArgs {
    #[clap(long)]
    pub file: Option<PathBuf>,
    #[clap(long)]
    pub private_key: Option<String>,
    #[clap(long)]
    pub gas_price: Option<String>,
    #[clap(long, help = "L1 RPC URL")]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub confirmations: Option<usize>,
}

#[derive(Debug)]
pub struct SendTransactionsArgsFinal {
    pub file: PathBuf,
    pub private_key: String,
    pub gas_price: String,
    pub l1_rpc_url: String,
    pub confirmations: usize,
}

impl SendTransactionsArgs {
    pub fn fill_values_with_prompt(self) -> SendTransactionsArgsFinal {
        let file = self
            .file
            .unwrap_or_else(|| Prompt::new("Path to transactions file").ask());

        let private_key = self
            .private_key
            .unwrap_or_else(|| Prompt::new("Secret key of the sender").ask());

        let gas_price = self
            .gas_price
            .unwrap_or_else(|| Prompt::new("Gas price").ask());

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            Prompt::new("L1 RPC URL")
                .default(LOCAL_RPC_URL)
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| "Invalid L1 RPC URL".to_string())
                })
                .ask()
        });

        let confirmations = self
            .confirmations
            .unwrap_or_else(|| Prompt::new("Confirmations").ask());

        SendTransactionsArgsFinal {
            file,
            private_key,
            gas_price,
            l1_rpc_url,
            confirmations,
        }
    }
}
