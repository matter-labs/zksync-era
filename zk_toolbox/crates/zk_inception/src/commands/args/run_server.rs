use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RunServerArgs {
    #[clap(long, help = "Components of server to run")]
    pub components: Option<Vec<String>>,
    #[clap(long, help = "Run server in genesis mode")]
    pub genesis: bool,
}
