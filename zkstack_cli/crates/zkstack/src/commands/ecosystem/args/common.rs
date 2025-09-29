use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
pub struct CommonEcosystemArgs {
    pub(crate) zksync_os: bool,
    pub(crate) update_submodules: bool,
    pub(crate) skip_build_dependencies: bool,
    pub(crate) l1_rpc_url: String,
}
