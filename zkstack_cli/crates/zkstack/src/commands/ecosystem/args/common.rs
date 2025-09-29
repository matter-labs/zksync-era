use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
pub struct CommonEcosystemArgs {
    #[clap(long, default_value_t = false, default_missing_value = "true")]
    pub(crate) zksync_os: bool,
    #[clap(long, default_value_t = false, default_missing_value = "true")]
    pub(crate) update_submodules: bool,
    #[clap(long, default_value_t = false, default_missing_value = "true")]
    pub(crate) skip_build_dependencies: bool,
    #[clap(long, help = "L1 RPC URL to use for transactions.")]
    pub(crate) l1_rpc_url: Option<String>,
}
