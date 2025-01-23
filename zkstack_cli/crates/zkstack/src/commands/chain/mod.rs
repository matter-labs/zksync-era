use ::zkstack_cli_common::forge::ForgeScriptArgs;
use args::build_transactions::BuildTransactionsArgs;
pub(crate) use args::create::ChainCreateArgsFinal;
use clap::{command, Subcommand};
pub(crate) use create::create_chain_inner;
use xshell::Shell;

use crate::commands::chain::{
    args::create::ChainCreateArgs, deploy_l2_contracts::Deploy2ContractsOption,
    genesis::GenesisCommand, init::ChainInitCommand,
};

mod accept_chain_ownership;
pub(crate) mod args;
mod build_transactions;
pub(crate) mod common;
#[cfg(feature = "gateway")]
pub(crate) mod convert_to_gateway;
pub(crate) mod create;
pub mod deploy_l2_contracts;
pub mod deploy_paymaster;
mod enable_evm_emulator;
#[cfg(feature = "gateway")]
mod gateway_upgrade;
pub mod genesis;
pub mod init;
#[cfg(feature = "gateway")]
mod migrate_from_gateway;
#[cfg(feature = "gateway")]
mod migrate_to_gateway;
pub mod register_chain;
mod set_token_multiplier_setter;
mod setup_legacy_bridge;
mod utils;

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
    /// Deploy L2 consensus registry
    #[command(alias = "consensus")]
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
    /// Prepare chain to be an eligible gateway
    #[cfg(feature = "gateway")]
    ConvertToGateway(ForgeScriptArgs),
    /// Migrate chain to gateway
    #[cfg(feature = "gateway")]
    MigrateToGateway(migrate_to_gateway::MigrateToGatewayArgs),
    /// Migrate chain from gateway
    #[cfg(feature = "gateway")]
    MigrateFromGateway(migrate_from_gateway::MigrateFromGatewayArgs),
    /// Upgrade to the protocol version that supports Gateway
    #[cfg(feature = "gateway")]
    GatewayUpgrade(gateway_upgrade::GatewayUpgradeArgs),
    /// Enable EVM emulation on chain (Not supported yet)
    EnableEvmEmulator(ForgeScriptArgs),
}

pub(crate) async fn run(shell: &Shell, args: ChainCommands) -> anyhow::Result<()> {
    match args {
        ChainCommands::Create(args) => create::run(args, shell),
        ChainCommands::Init(args) => init::run(*args, shell).await,
        ChainCommands::BuildTransactions(args) => build_transactions::run(args, shell).await,
        ChainCommands::Genesis(args) => genesis::run(args, shell).await,
        ChainCommands::RegisterChain(args) => register_chain::run(args, shell).await,
        ChainCommands::DeployL2Contracts(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::All).await
        }
        ChainCommands::AcceptChainOwnership(args) => accept_chain_ownership::run(args, shell).await,
        ChainCommands::DeployConsensusRegistry(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::ConsensusRegistry).await
        }
        ChainCommands::DeployMulticall3(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::Multicall3).await
        }
        ChainCommands::DeployTimestampAsserter(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::TimestampAsserter).await
        }
        ChainCommands::DeployUpgrader(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::Upgrader).await
        }
        ChainCommands::DeployPaymaster(args) => deploy_paymaster::run(args, shell).await,
        ChainCommands::UpdateTokenMultiplierSetter(args) => {
            set_token_multiplier_setter::run(args, shell).await
        }
        #[cfg(feature = "gateway")]
        ChainCommands::ConvertToGateway(args) => convert_to_gateway::run(args, shell).await,
        #[cfg(feature = "gateway")]
        ChainCommands::MigrateToGateway(args) => migrate_to_gateway::run(args, shell).await,
        #[cfg(feature = "gateway")]
        ChainCommands::MigrateFromGateway(args) => migrate_from_gateway::run(args, shell).await,
        #[cfg(feature = "gateway")]
        ChainCommands::GatewayUpgrade(args) => gateway_upgrade::run(args, shell).await,
        ChainCommands::EnableEvmEmulator(args) => enable_evm_emulator::run(args, shell).await,
    }
}
