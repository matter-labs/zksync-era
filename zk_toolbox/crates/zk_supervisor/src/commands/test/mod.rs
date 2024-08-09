use args::{
    all::AllArgs, integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs,
    upgrade::UpgradeArgs,
};
use clap::Subcommand;
use xshell::Shell;

use crate::messages::{
    MSG_ALL_TEST_ABOUT, MSG_BUILD_ABOUT, MSG_INTEGRATION_TESTS_ABOUT, MSG_RECOVERY_TEST_ABOUT,
    MSG_REVERT_TEST_ABOUT, MSG_UPGRADE_TEST_ABOUT,
};

mod all;
mod args;
mod build;
mod integration;
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
    Upgrade(UpgradeArgs),
    #[clap(about = MSG_ALL_TEST_ABOUT)]
    All(AllArgs),
    #[clap(about = MSG_BUILD_ABOUT)]
    Build,
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration(args) => integration::run(shell, args),
        TestCommands::Revert(args) => revert::run(shell, args),
        TestCommands::Recovery(args) => recovery::run(shell, args),
        TestCommands::Upgrade(args) => upgrade::run(shell, args),
        TestCommands::All(args) => all::run(shell, args),
        TestCommands::Build => build::run(shell),
    }
}
