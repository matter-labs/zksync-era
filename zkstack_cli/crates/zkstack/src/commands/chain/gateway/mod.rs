use clap::Subcommand;
use gateway_common::MigrationDirection;
use grant_gateway_whitelist::GrantGatewayWhitelistCalldataArgs;
use xshell::Shell;
use zkstack_cli_common::forge::ForgeScriptArgs;

mod constants;
pub(crate) mod convert_to_gateway;
pub(crate) mod create_tx_filterer;
pub(crate) mod finalize_chain_migration_from_gw;
pub(crate) mod finalize_chain_migration_to_gateway;
pub(crate) mod gateway_common;
pub(crate) mod grant_gateway_whitelist;
mod messages;
mod migrate_from_gateway;
mod migrate_from_gateway_calldata;
pub mod migrate_to_gateway;
pub(crate) mod migrate_to_gateway_calldata;
mod notify_server_calldata;

#[derive(Subcommand, Debug)]
pub enum GatewayComamnds {
    GrantGatewayTransactionFiltererWhitelistCalldata(GrantGatewayWhitelistCalldataArgs),
    NotifyAboutToGatewayUpdateCalldata(notify_server_calldata::NotifyServerCalldataArgs),
    NotifyAboutFromGatewayUpdateCalldata(notify_server_calldata::NotifyServerCalldataArgs),
    MigrateToGatewayCalldata(migrate_to_gateway_calldata::MigrateToGatewayCalldataArgs),
    MigrateFromGatewayCalldata(migrate_from_gateway_calldata::MigrateFromGatewayCalldataArgs),
    FinalizeChainMigrationFromGateway(
        finalize_chain_migration_from_gw::FinalizeChainMigrationFromGatewayArgs,
    ),
    /// Deploy tx filterer and set it for gateway
    CreateTxFilterer(ForgeScriptArgs),
    /// Prepare chain to be an eligible gateway
    ConvertToGateway(convert_to_gateway::ConvertToGatewayArgs),
    /// Migrate chain to gateway
    MigrateToGateway(migrate_to_gateway::MigrateToGatewayArgs),
    // Finalize chain migration to gateway by sending the remaining priority txs
    FinalizeChainMigrationToGateway(
        finalize_chain_migration_to_gateway::FinalizeChainMigrationToGatewayArgs,
    ),
    /// Migrate chain from gateway
    MigrateFromGateway(migrate_from_gateway::MigrateFromGatewayArgs),
    NotifyAboutToGatewayUpdate(ForgeScriptArgs),
    NotifyAboutFromGatewayUpdate(ForgeScriptArgs),
}

pub async fn run(shell: &Shell, args: GatewayComamnds) -> anyhow::Result<()> {
    match args {
        GatewayComamnds::GrantGatewayTransactionFiltererWhitelistCalldata(args) => {
            grant_gateway_whitelist::run(shell, args).await
        }
        GatewayComamnds::NotifyAboutToGatewayUpdateCalldata(args) => {
            notify_server_calldata::run(shell, args, MigrationDirection::ToGateway).await
        }
        GatewayComamnds::MigrateToGatewayCalldata(args) => {
            migrate_to_gateway_calldata::run(shell, args).await
        }
        GatewayComamnds::MigrateFromGatewayCalldata(args) => {
            migrate_from_gateway_calldata::run(shell, args).await
        }
        GatewayComamnds::FinalizeChainMigrationFromGateway(args) => {
            finalize_chain_migration_from_gw::run(shell, args).await
        }
        GatewayComamnds::NotifyAboutFromGatewayUpdateCalldata(args) => {
            notify_server_calldata::run(shell, args, MigrationDirection::FromGateway).await
        }
        GatewayComamnds::CreateTxFilterer(args) => create_tx_filterer::run(args, shell).await,
        GatewayComamnds::ConvertToGateway(args) => convert_to_gateway::run(args, shell).await,
        GatewayComamnds::MigrateToGateway(args) => migrate_to_gateway::run(args, shell).await,
        GatewayComamnds::FinalizeChainMigrationToGateway(args) => {
            finalize_chain_migration_to_gateway::run(args, shell).await
        }
        GatewayComamnds::MigrateFromGateway(args) => migrate_from_gateway::run(args, shell).await,
        GatewayComamnds::NotifyAboutToGatewayUpdate(args) => {
            gateway_common::notify_server(args, shell, MigrationDirection::ToGateway).await
        }
        GatewayComamnds::NotifyAboutFromGatewayUpdate(args) => {
            gateway_common::notify_server(args, shell, MigrationDirection::FromGateway).await
        }
    }
}
