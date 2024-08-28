use clap::Parser;

use crate::messages::MSG_TEST_RUST_OPTIONS_HELP;

#[derive(Debug, Parser)]
pub struct RustArgs {
    #[clap(long, help = MSG_TEST_RUST_OPTIONS_HELP)]
    pub options: Option<String>,
    #[clap(long)]
    pub link_to_code: Option<String>,
    #[clap(long)]
    pub test_server_url: Option<String>,
    #[clap(long)]
    pub test_prover_url: Option<String>,
}
