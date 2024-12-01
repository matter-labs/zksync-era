use clap::Subcommand;
use config::ChainConfig;
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

pub(crate) async fn run(
    shell: &Shell,
    args: ContractVerifierCommands,
    chain: ChainConfig,
) -> anyhow::Result<()> {
    match args {
        ContractVerifierCommands::Build => build::build(shell, chain).await,
        ContractVerifierCommands::Run => run::run(shell, chain).await,
        ContractVerifierCommands::Wait(args) => wait::wait(args, chain).await,
        ContractVerifierCommands::Init(args) => init::run(shell, args, chain).await,
    }
}
