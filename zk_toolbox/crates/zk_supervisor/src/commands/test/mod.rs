use args::{integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs};
use clap::Subcommand;
use xshell::Shell;

use crate::messages::{
    MSG_INTEGRATION_TESTS_ABOUT, MSG_L1_CONTRACTS_ABOUT, MSG_RECOVERY_TEST_ABOUT,
    MSG_REVERT_TEST_ABOUT, MSG_UPGRADE_TEST_ABOUT,
};

mod args;
mod integration;
mod l1_contracts;
mod recovery;
mod revert;
mod upgrade;

#[derive(Subcommand, Debug)]
pub enum TestCommands {
    #[clap(about = MSG_INTEGRATION_TESTS_ABOUT, alias = "i")]
    Integration(IntegrationArgs),
    #[clap(about = MSG_REVERT_TEST_ABOUT, alias = "r")]
    Revert(RevertArgs),
    #[clap(about = MSG_RECOVERY_TEST_ABOUT, alias = "rec")]
    Recovery(RecoveryArgs),
    #[clap(about = MSG_UPGRADE_TEST_ABOUT, alias = "u")]
    Upgrade,
    #[clap(about = MSG_L1_CONTRACTS_ABOUT, alias = "l1")]
    L1Contracts,
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration(args) => integration::run(shell, args),
        TestCommands::Revert(args) => revert::run(shell, args),
        TestCommands::Recovery(args) => recovery::run(shell, args),
        TestCommands::Upgrade => upgrade::run(shell),
        TestCommands::L1Contracts => l1_contracts::run(shell),
    }
}
