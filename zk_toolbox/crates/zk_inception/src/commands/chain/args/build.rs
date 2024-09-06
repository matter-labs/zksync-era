use std::path::PathBuf;

use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use types::L1Network;
use url::Url;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT},
};

const DEFAULT_OUT_DIR: &str = "transactions/chain";

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct ChainBuildArgs {
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

impl ChainBuildArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> ChainBuildArgsFinal {
        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_L1_RPC_URL_PROMPT);
            if config.l1_network == L1Network::Localhost {
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

        ChainBuildArgsFinal {
            out: self.out.unwrap_or_else(|| DEFAULT_OUT_DIR.into()),
            forge_args: self.forge_args,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChainBuildArgsFinal {
    pub out: PathBuf,
    pub forge_args: ForgeScriptArgs,
    pub l1_rpc_url: String,
}
