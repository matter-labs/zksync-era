use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct AllArgs {
    #[clap(long)]
    pub db_url: Url,
    #[clap(long)]
    pub db_name: String,
    #[clap(long)]
    pub l1_rpc_url: String,
}
