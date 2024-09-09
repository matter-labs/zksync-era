use std::path::PathBuf;

use clap::Parser;
use common::Prompt;

#[derive(Debug, Parser)]
pub struct SendTransactionsArgs {
    #[clap(long)]
    pub file: Option<PathBuf>,
    #[clap(long)]
    pub private_key: Option<String>,
    #[clap(long)]
    pub gas_price: Option<String>,
}

#[derive(Debug)]
pub struct SendTransactionsArgsFinal {
    pub file: PathBuf,
    pub private_key: String,
    pub gas_price: String,
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

        SendTransactionsArgsFinal {
            file,
            private_key,
            gas_price,
        }
    }
}
