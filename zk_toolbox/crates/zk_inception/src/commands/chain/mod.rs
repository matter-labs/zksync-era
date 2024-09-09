use args::build_transactions::BuildTransactionsArgs;
pub(crate) use args::create::ChainCreateArgsFinal;
use clap::Subcommand;
use common::forge::ForgeScriptArgs;
pub(crate) use create::create_chain_inner;
use xshell::Shell;

use crate::commands::chain::{
    args::{create::ChainCreateArgs, genesis::GenesisArgs, init::InitArgs},
    deploy_l2_contracts::Deploy2ContractsOption,
};

pub(crate) mod args;
mod build_transactions;
mod create;
pub mod deploy_l2_contracts;
pub mod deploy_paymaster;
pub mod genesis;
pub(crate) mod init;
mod set_token_multiplier_setter;

#[derive(Subcommand, Debug)]
pub enum ChainCommands {
    /// Create a new chain, setting the necessary configurations for later initialization
    Create(ChainCreateArgs),
    /// Create unsigned transactions for chain deployment
    BuildTransactions(BuildTransactionsArgs),
    /// Initialize chain, deploying necessary contracts and performing on-chain operations
    Init(InitArgs),
    /// Run server genesis
    Genesis(GenesisArgs),
    /// Initialize bridges on l2
    #[command(alias = "bridge")]
    InitializeBridges(ForgeScriptArgs),
    /// Deploy all l2 contracts
    #[command(alias = "l2")]
    DeployL2Contracts(ForgeScriptArgs),
    /// Deploy L2 consensus registry
    #[command(alias = "consensus")]
    DeployConsensusRegistry(ForgeScriptArgs),
    /// Deploy Default Upgrader
    Upgrader(ForgeScriptArgs),
    /// Deploy paymaster smart contract
    #[command(alias = "paymaster")]
    DeployPaymaster(ForgeScriptArgs),
    /// Update Token Multiplier Setter address on L1
    UpdateTokenMultiplierSetter(ForgeScriptArgs),
}

pub(crate) async fn run(shell: &Shell, args: ChainCommands) -> anyhow::Result<()> {
    match args {
        ChainCommands::Create(args) => create::run(args, shell),
        ChainCommands::Init(args) => init::run(args, shell).await,
        ChainCommands::BuildTransactions(args) => build_transactions::run(args, shell).await,
        ChainCommands::Genesis(args) => genesis::run(args, shell).await,
        ChainCommands::DeployL2Contracts(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::All).await
        }
        ChainCommands::DeployConsensusRegistry(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::ConsensusRegistry).await
        }
        ChainCommands::Upgrader(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::Upgrader).await
        }
        ChainCommands::InitializeBridges(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::InitiailizeBridges).await
        }
        ChainCommands::DeployPaymaster(args) => deploy_paymaster::run(args, shell).await,
        ChainCommands::UpdateTokenMultiplierSetter(args) => {
            set_token_multiplier_setter::run(args, shell).await
        }
    }
}
