use args::init::InitContractVerifierArgs;
use clap::Subcommand;
use config::ChainConfig;
use xshell::Shell;

mod args;
mod init;
mod run;

#[derive(Subcommand, Debug)]
pub enum ContractVerifierCommands {
    /// Run contract verifier
    Run,
    /// Download required binaries for contract verifier
    Init(InitContractVerifierArgs),
}

pub(crate) async fn run(
    shell: &Shell,
    args: ContractVerifierCommands,
    chain: ChainConfig,
) -> anyhow::Result<()> {
    match args {
        ContractVerifierCommands::Run => run::run(shell, chain).await,
        ContractVerifierCommands::Init(args) => init::run(shell, args, chain).await,
    }
}
