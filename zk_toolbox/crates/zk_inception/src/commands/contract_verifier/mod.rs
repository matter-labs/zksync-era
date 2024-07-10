use clap::Subcommand;
use xshell::Shell;

pub mod run;

#[derive(Subcommand, Debug)]
pub enum ContractVerifierCommands {
    /// Run contract verifier
    Run,
}

pub(crate) async fn run(shell: &Shell, args: ContractVerifierCommands) -> anyhow::Result<()> {
    match args {
        ContractVerifierCommands::Run => run::run(shell).await,
    }
}
