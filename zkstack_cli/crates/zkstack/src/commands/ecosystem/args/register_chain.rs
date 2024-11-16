use std::path::PathBuf;

use clap::Parser;
use common::forge::ForgeScriptArgs;
use ethers::abi::Address;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::{
    defaults::LOCAL_RPC_URL,
    messages::{MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT},
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct RegisterChainArgs {
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long)]
    pub chain_id: Option<L2ChainId>,
    #[clap(long)]
    pub dev: bool,
    #[clap(long)]
    pub proposal_author: Option<Address>,
    #[clap(flatten)]
    pub forge_script_args: ForgeScriptArgs,
    #[clap(long)]
    pub no_broadcast: bool,
    #[arg(long, short)]
    pub out: Option<PathBuf>,
}
