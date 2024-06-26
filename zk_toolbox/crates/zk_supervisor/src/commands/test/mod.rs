use args::{integration::IntegrationArgs, revert::RevertArgs};
use clap::Subcommand;
use xshell::Shell;

use crate::messages::{MSG_INTEGRATION_TESTS_ABOUT, MSG_REVERT_TEST_ABOUT};

mod args;
mod integration;
mod revert;

#[derive(Subcommand, Debug)]
pub enum TestCommands {
    #[clap(about = MSG_INTEGRATION_TESTS_ABOUT)]
    Integration(IntegrationArgs),
    #[clap(about = MSG_REVERT_TEST_ABOUT)]
    Revert(RevertArgs),
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration(args) => integration::run(shell, args),
        TestCommands::Revert(args) => revert::run(shell, args),
    }
}
