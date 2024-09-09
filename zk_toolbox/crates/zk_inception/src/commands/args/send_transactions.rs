use std::path::PathBuf;

use clap::Parser;
use common::Prompt;

#[derive(Debug, Parser)]
pub struct SendTransactionsArgs {
    #[clap(long)]
    pub file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct SendTransactionsArgsFinal {
    pub file: PathBuf,
}

impl SendTransactionsArgs {
    pub fn fill_values_with_prompt(self) -> SendTransactionsArgsFinal {
        let file = self
            .file
            .unwrap_or_else(|| Prompt::new("Path to transactions file").ask());

        SendTransactionsArgsFinal { file }
    }
}
