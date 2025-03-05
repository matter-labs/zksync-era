use clap::Subcommand;
use xshell::Shell;

use self::args::init::InitContractVerifierArgs;
use crate::commands::args::WaitArgs;

mod args;
mod build;
mod init;
mod run;
mod wait;

#[derive(Subcommand, Debug)]
pub enum ContractVerifierCommands {
    /// Build contract verifier binary
    Build,
    /// Run contract verifier
    Run,
    /// Wait for contract verifier to start
    Wait(WaitArgs),
    /// Download required binaries for contract verifier
    Init(InitContractVerifierArgs),
}

pub(crate) async fn run(shell: &Shell, args: ContractVerifierCommands) -> anyhow::Result<()> {
    match args {
        ContractVerifierCommands::Build => build::build(shell).await,
        ContractVerifierCommands::Run => run::run(shell).await,
        ContractVerifierCommands::Wait(args) => wait::wait(shell, args).await,
        ContractVerifierCommands::Init(args) => init::run(shell, args).await,
    }
}
