use clap::Parser;
use serde::{Deserialize, Serialize};
use zkstack_cli_common::forge::ForgeScriptArgs;

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct SetPubdataPricingModeArgs {
    /// Whether set pubdata to rollup or validium (if false)
    #[arg(long, short)]
    pub rollup: Option<bool>,
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
}
