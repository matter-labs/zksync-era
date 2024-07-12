use args::init::InitContractVerifierArgs;
use clap::Subcommand;
use xshell::Shell;

pub mod args;
pub mod init;
pub mod run;

#[derive(Subcommand, Debug)]
pub enum ContractVerifierCommands {
    /// Run contract verifier
    Run,
    /// Download required binaries for contract verifier
    Init(InitContractVerifierArgs),
}

pub(crate) async fn run(shell: &Shell, args: ContractVerifierCommands) -> anyhow::Result<()> {
    match args {
        ContractVerifierCommands::Run => run::run(shell).await,
        ContractVerifierCommands::Init(args) => init::run(shell, args).await,
    }
}
