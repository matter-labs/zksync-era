use args::init::InitContractVerifierArgs;
use clap::Subcommand;
use xshell::Shell;

mod args;
mod build;
mod init;
mod run;

#[derive(Subcommand, Debug)]
pub enum ContractVerifierCommands {
    /// Build contract verifier binary
    Build,
    /// Run contract verifier
    Run,
    /// Download required binaries for contract verifier
    Init(InitContractVerifierArgs),
}

pub(crate) async fn run(shell: &Shell, args: ContractVerifierCommands) -> anyhow::Result<()> {
    match args {
        ContractVerifierCommands::Build => build::build(shell).await,
        ContractVerifierCommands::Run => run::run(shell).await,
        ContractVerifierCommands::Init(args) => init::run(shell, args).await,
    }
}
