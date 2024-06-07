use clap::Subcommand;
use xshell::Shell;

use crate::messages::MSG_TEST_INTEGRATION_ABOUT;

mod integration;

#[derive(Subcommand, Debug)]
pub enum TestCommands {
    #[clap(about = MSG_TEST_INTEGRATION_ABOUT)]
    Integration,
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration => integration::run(shell),
    }
}
