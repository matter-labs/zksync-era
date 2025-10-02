pub mod args;
pub(crate) mod commands;

use clap::Subcommand;
use xshell::Shell;

use crate::commands::ctm::{
    args::{InitNewCTMArgs, RegisterCTMArgs, SetNewCTMArgs},
    commands::{init_new_ctm, register_ctm, set_new_ctm_contracts},
};

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum CTMCommands {
    /// Initialize new ecosystem on existing bridgehub
    InitNewCTM(InitNewCTMArgs),
    /// Initialize ecosystem core contracts
    RegisterCTM(RegisterCTMArgs),
    /// Set contracts and configs for future commands
    SetCTMContracts(SetNewCTMArgs),
}

pub(crate) async fn run(shell: &Shell, args: CTMCommands) -> anyhow::Result<()> {
    match args {
        CTMCommands::InitNewCTM(args) => init_new_ctm::run(args, shell).await,
        CTMCommands::RegisterCTM(args) => register_ctm::run(args, shell).await,
        CTMCommands::SetCTMContracts(args) => set_new_ctm_contracts::run(args, shell),
    }
}
