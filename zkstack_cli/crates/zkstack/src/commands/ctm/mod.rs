pub mod args;
pub(crate) mod commands;

use clap::Subcommand;
use xshell::Shell;

use crate::commands::ctm::{
    args::{InitNewCTMArgs, SetNewCTMArgs},
    commands::{init_new_ctm, set_new_ctm_contracts},
};

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum CTMCommands {
    /// Initialize new CTM on existing bridgehub
    InitNewCTM(InitNewCTMArgs),
    /// Specify source contracts and configs for the specific CTM.
    /// These sources will be used in the future when deploying new CTM.
    SetCTMContracts(SetNewCTMArgs),
}

pub(crate) async fn run(shell: &Shell, args: CTMCommands) -> anyhow::Result<()> {
    match args {
        CTMCommands::InitNewCTM(args) => init_new_ctm::run(args, shell).await,
        CTMCommands::SetCTMContracts(args) => set_new_ctm_contracts::run(args, shell),
    }
}
