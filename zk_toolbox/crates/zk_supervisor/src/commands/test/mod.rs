use args::revert_and_restart::RevertAndRestartArgs;
use clap::Subcommand;
use integration::IntegrationTestArgs;
use xshell::Shell;

use crate::messages::{MSG_TEST_INTEGRATION_ABOUT, MSG_TEST_REVERT_AND_RESTART_ABOUT};

mod args;
mod integration;
mod revert;

#[derive(Subcommand, Debug)]
pub enum TestCommands {
    #[clap(about = MSG_TEST_INTEGRATION_ABOUT)]
    Integration(IntegrationTestArgs),
    #[clap(about = MSG_TEST_REVERT_AND_RESTART_ABOUT)]
    RevertAndRestart(RevertAndRestartArgs),
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration(args) => integration::run(shell, args),
        TestCommands::RevertAndRestart(args) => revert::run(shell, args),
    }
}
