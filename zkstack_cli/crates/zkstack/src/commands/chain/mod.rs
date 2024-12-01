use ::common::forge::ForgeScriptArgs;
use anyhow::Context;
pub(crate) use args::create::ChainCreateArgsFinal;
use args::{build_transactions::BuildTransactionsArgs, run_server::ServerArgs};
use clap::{command, Subcommand};
use config::zkstack_config::ZkStackConfig;
use consensus::ConsensusCommand;
use contract_verifier::ContractVerifierCommands;
pub(crate) use create::create_chain_inner;
use xshell::Shell;

use crate::{
    commands::chain::{
        args::create::ChainCreateArgs, deploy_l2_contracts::Deploy2ContractsOption,
        genesis::GenesisCommand, init::ChainInitCommand,
    },
    messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_ECOSYSTEM_CONFIG_INVALID_ERR},
};

mod accept_chain_ownership;
pub(crate) mod args;
mod build_transactions;
mod common;
mod consensus;
pub mod contract_verifier;
mod create;
pub mod deploy_l2_contracts;
pub mod deploy_paymaster;
pub mod genesis;
pub mod init;
pub mod register_chain;
mod server;
mod set_token_multiplier_setter;
mod setup_legacy_bridge;

#[derive(Subcommand, Debug)]
pub enum ChainCommands {
    /// Create a new chain, setting the necessary configurations for later initialization
    Create(ChainCreateArgs),
    /// Create unsigned transactions for chain deployment
    BuildTransactions(BuildTransactionsArgs),
    /// Initialize chain, deploying necessary contracts and performing on-chain operations
    Init(Box<ChainInitCommand>),
    /// Run server genesis
    Genesis(GenesisCommand),
    /// Register a new chain on L1 (executed by L1 governor).
    /// This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts,
    /// registers chain with BridgeHub and sets pending admin for DiamondProxy.
    /// Note: After completion, L2 governor can accept ownership by running `accept-chain-ownership`
    #[command(alias = "register")]
    RegisterChain(ForgeScriptArgs),
    /// Deploy all L2 contracts (executed by L1 governor).
    #[command(alias = "l2")]
    DeployL2Contracts(ForgeScriptArgs),
    /// Accept ownership of L2 chain (executed by L2 governor).
    /// This command should be run after `register-chain` to accept ownership of newly created
    /// DiamondProxy contract.
    #[command(alias = "accept-ownership")]
    AcceptChainOwnership(ForgeScriptArgs),
    /// Initialize bridges on L2
    #[command(alias = "bridge")]
    InitializeBridges(ForgeScriptArgs),
    /// Deploy L2 consensus registry
    DeployConsensusRegistry(ForgeScriptArgs),
    /// Deploy L2 multicall3
    #[command(alias = "multicall3")]
    DeployMulticall3(ForgeScriptArgs),
    /// Deploy L2 TimestampAsserter
    #[command(alias = "timestamp-asserter")]
    DeployTimestampAsserter(ForgeScriptArgs),
    /// Deploy Default Upgrader
    #[command(alias = "upgrader")]
    DeployUpgrader(ForgeScriptArgs),
    /// Deploy paymaster smart contract
    #[command(alias = "paymaster")]
    DeployPaymaster(ForgeScriptArgs),
    /// Update Token Multiplier Setter address on L1
    UpdateTokenMultiplierSetter(ForgeScriptArgs),
    /// Run server
    Server(ServerArgs),
    /// Run contract verifier
    #[command(subcommand)]
    ContractVerifier(ContractVerifierCommands),
    #[command(subcommand)]
    Consensus(ConsensusCommand),
}

pub(crate) async fn run(shell: &Shell, cmd: ChainCommands) -> anyhow::Result<()> {
    if let ChainCommands::Create(args) = cmd {
        return create::run(args, shell);
    }

    let chain = ZkStackConfig::current_chain(shell).context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let ecosystem = ZkStackConfig::ecosystem(shell).ok();

    match cmd {
        ChainCommands::Init(args) => init::run(*args, shell, chain, ecosystem).await,
        ChainCommands::BuildTransactions(args) => {
            build_transactions::run(args, shell, chain, ecosystem).await
        }
        ChainCommands::Genesis(args) => genesis::run(args, shell, chain).await,
        ChainCommands::RegisterChain(args) => register_chain::run(args, shell).await,
        ChainCommands::DeployL2Contracts(args) => {
            deploy_l2_contracts::run(
                args,
                shell,
                Deploy2ContractsOption::All,
                chain,
                ecosystem.context(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)?,
            )
            .await
        }
        ChainCommands::AcceptChainOwnership(args) => {
            accept_chain_ownership::run(args, shell, chain).await
        }
        ChainCommands::DeployConsensusRegistry(args) => {
            deploy_l2_contracts::run(
                args,
                shell,
                Deploy2ContractsOption::ConsensusRegistry,
                chain,
                ecosystem.context(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)?,
            )
            .await
        }
        ChainCommands::DeployMulticall3(args) => {
            deploy_l2_contracts::run(
                args,
                shell,
                Deploy2ContractsOption::Multicall3,
                chain,
                ecosystem.context(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)?,
            )
            .await
        }
        ChainCommands::DeployTimestampAsserter(args) => {
            deploy_l2_contracts::run(
                args,
                shell,
                Deploy2ContractsOption::TimestampAsserter,
                chain,
                ecosystem.context(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)?,
            )
            .await
        }
        ChainCommands::DeployUpgrader(args) => {
            deploy_l2_contracts::run(
                args,
                shell,
                Deploy2ContractsOption::Upgrader,
                chain,
                ecosystem.context(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)?,
            )
            .await
        }
        ChainCommands::InitializeBridges(args) => {
            deploy_l2_contracts::run(
                args,
                shell,
                Deploy2ContractsOption::InitiailizeBridges,
                chain,
                ecosystem.context(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)?,
            )
            .await
        }
        ChainCommands::DeployPaymaster(args) => deploy_paymaster::run(args, shell, chain).await,
        ChainCommands::UpdateTokenMultiplierSetter(args) => {
            set_token_multiplier_setter::run(args, shell, chain).await
        }
        ChainCommands::Server(args) => server::run(shell, args, chain).await,
        ChainCommands::ContractVerifier(args) => contract_verifier::run(shell, args, chain).await,
        ChainCommands::Consensus(cmd) => cmd.run(shell).await,
        ChainCommands::Create(_) => unreachable!("Chain create is handled before loading chain"),
    }
}
