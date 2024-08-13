use args::{
    integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs, rust::RustArgs,
};
use clap::Subcommand;
use xshell::Shell;

use crate::messages::{
    MSG_INTEGRATION_TESTS_ABOUT, MSG_L1_CONTRACTS_ABOUT, MSG_PROVER_TEST_ABOUT,
    MSG_RECOVERY_TEST_ABOUT, MSG_REVERT_TEST_ABOUT, MSG_RUST_TEST_ABOUT, MSG_UPGRADE_TEST_ABOUT,
};

mod args;
mod integration;
mod l1_contracts;
mod prover;
mod recovery;
mod revert;
mod rust;
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
    #[clap(about = MSG_RUST_TEST_ABOUT, alias = "unit")]
    Rust(RustArgs),
    #[clap(about = MSG_L1_CONTRACTS_ABOUT, alias = "l1")]
    L1Contracts,
    #[clap(about = MSG_PROVER_TEST_ABOUT, alias = "p")]
    Prover,
}

pub fn run(shell: &Shell, args: TestCommands) -> anyhow::Result<()> {
    match args {
        TestCommands::Integration(args) => integration::run(shell, args),
        TestCommands::Revert(args) => revert::run(shell, args),
        TestCommands::Recovery(args) => recovery::run(shell, args),
        TestCommands::Upgrade => upgrade::run(shell),
        TestCommands::Rust(args) => rust::run(shell, args),
        TestCommands::L1Contracts => l1_contracts::run(shell),
        TestCommands::Prover => prover::run(shell),
    }
}
