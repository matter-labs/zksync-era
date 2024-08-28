use clap::Parser;

use crate::messages::{
    MSG_TEST_PROVER_URL_HELP, MSG_TEST_RUST_LINK_TO_CODE_HELP, MSG_TEST_RUST_OPTIONS_HELP,
    MSG_TEST_SERVER_URL_HELP,
};

#[derive(Debug, Parser)]
pub struct RustArgs {
    #[clap(long, help = MSG_TEST_RUST_OPTIONS_HELP)]
    pub options: Option<String>,
    #[clap(long, help = MSG_TEST_RUST_LINK_TO_CODE_HELP)]
    pub link_to_code: Option<String>,
    #[clap(long, help = MSG_TEST_SERVER_URL_HELP)]
    pub test_server_url: Option<String>,
    #[clap(long, help = MSG_TEST_PROVER_URL_HELP)]
    pub test_prover_url: Option<String>,
}
