use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct PortalArgs {
    #[clap(
        long,
        default_value = "3000",
        help = "The port number for the portal app"
    )]
    pub port: u16,
}
