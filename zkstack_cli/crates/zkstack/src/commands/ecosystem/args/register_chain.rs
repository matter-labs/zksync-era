use std::path::PathBuf;

use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt};
use config::{ChainConfig, EcosystemConfig};
use ethers::abi::Address;
use serde::{Deserialize, Serialize};
use url::Url;
use zksync_basic_types::L2ChainId;

use crate::{
    commands::chain::args::genesis::GenesisArgsFinal,
    consts::DEFAULT_UNSIGNED_TRANSACTIONS_DIR,
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT},
};
const CHAIN_SUBDIR: &str = "chain";

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct RegisterChainArgs {
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub chain_id: Option<L2ChainId>,
    #[clap(long)]
    pub proposal_author: Option<Address>,
    #[clap(flatten)]
    pub forge_script_args: ForgeScriptArgs,
    #[clap(long)]
    pub no_broadcast: bool,
    #[arg(long, short)]
    pub out: Option<PathBuf>,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub dev: bool,
}

impl RegisterChainArgs {
    pub fn fill_values_with_prompt(
        self,
        default_chain_id: L2ChainId,
        chain_governor: Address,
    ) -> RegisterChainArgsFinal {
        let default_out = PathBuf::from(DEFAULT_UNSIGNED_TRANSACTIONS_DIR).join(CHAIN_SUBDIR);
        if self.dev {
            let chain_id_str = &format!("{:?}", default_chain_id);
            return RegisterChainArgsFinal {
                l1_rpc_url: LOCAL_RPC_URL.to_string(),
                chain_id: default_chain_id,
                proposal_author: chain_governor,
                out: default_out.join(chain_id_str),
                forge_script_args: Default::default(),
                no_broadcast: false,
            };
        }

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

        let chain_id = self.chain_id.unwrap_or_else(|| {
            Prompt::new("Chain id ")
                .default(&format!("{:?}", default_chain_id))
                .ask()
        });

        let proposal_author = self.proposal_author.unwrap_or_else(|| {
            Prompt::new("Proposal author ")
                .default(&format!("{:?}", chain_governor))
                .ask()
        });
        let chain_id_str = &format!("{:?}", chain_id);

        RegisterChainArgsFinal {
            l1_rpc_url,
            chain_id,
            proposal_author,
            out: self
                .out
                .unwrap_or(PathBuf::from(DEFAULT_UNSIGNED_TRANSACTIONS_DIR).join(CHAIN_SUBDIR))
                .join(chain_id_str),
            forge_script_args: self.forge_script_args,
            no_broadcast: self.no_broadcast,
        }
    }
}

pub struct RegisterChainArgsFinal {
    pub l1_rpc_url: String,
    pub chain_id: L2ChainId,
    pub proposal_author: Address,
    pub out: PathBuf,
    pub forge_script_args: ForgeScriptArgs,
    pub no_broadcast: bool,
}
