use clap::Parser;

use crate::messages::MSG_TEST_RUST_OPTIONS_HELP;

#[derive(Debug, Parser)]
pub struct RustArgs {
    #[clap(long, help = MSG_TEST_RUST_OPTIONS_HELP)]
    pub options: Option<String>,
}
