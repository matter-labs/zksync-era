use args::{integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs};
use clap::Subcommand;
use xshell::Shell;

use crate::messages::{
    MSG_INTEGRATION_TESTS_ABOUT, MSG_RECOVERY_TEST_ABOUT, MSG_REVERT_TEST_ABOUT,
};

mod args;
mod integration;
mod recovery;
mod revert;

#[derive(Subcommand, Debug)]
pub enum TestCommands {
    #[clap(about = MSG_INTEGRATION_TESTS_ABOUT, alias = "i")]
    Integration(IntegrationArgs),
    #[clap(about = MSG_REVERT_TEST_ABOUT, alias = "r")]
    Revert(RevertArgs),
    #[clap(about = MSG_RECOVERY_TEST_ABOUT, alias = "rec")]
    Recovery(RecoveryArgs),
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration(args) => integration::run(shell, args),
        TestCommands::Revert(args) => revert::run(shell, args),
        TestCommands::Recovery(args) => recovery::run(shell, args),
    }
}
