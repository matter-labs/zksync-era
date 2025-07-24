use clap::Subcommand;
use gateway_common::MigrationDirection;
use grant_gateway_whitelist::GrantGatewayWhitelistCalldataArgs;
use xshell::Shell;
use zkstack_cli_common::forge::ForgeScriptArgs;

mod constants;
pub(crate) mod convert_to_gateway;
pub(crate) mod finalize_chain_migration_from_gw;
pub(crate) mod gateway_common;
pub(crate) mod grant_gateway_whitelist;
mod messages;
mod migrate_from_gateway;
mod migrate_from_gateway_calldata;
pub mod migrate_to_gateway;
pub(crate) mod migrate_to_gateway_calldata;
pub(crate) mod migrate_token_balances;
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
    /// Prepare chain to be an eligible gateway
    ConvertToGateway(ForgeScriptArgs),
    /// Migrate chain to gateway
    MigrateToGateway(migrate_to_gateway::MigrateToGatewayArgs),
    /// Migrate chain from gateway
    MigrateFromGateway(migrate_from_gateway::MigrateFromGatewayArgs),
    NotifyAboutToGatewayUpdate(ForgeScriptArgs),
    NotifyAboutFromGatewayUpdate(ForgeScriptArgs),
    MigrateTokenBalances(migrate_token_balances::MigrateTokenBalancesArgs),
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
        GatewayComamnds::ConvertToGateway(args) => convert_to_gateway::run(args, shell).await,
        GatewayComamnds::MigrateToGateway(args) => migrate_to_gateway::run(args, shell).await,
        GatewayComamnds::MigrateFromGateway(args) => migrate_from_gateway::run(args, shell).await,
        GatewayComamnds::NotifyAboutToGatewayUpdate(args) => {
            gateway_common::notify_server(args, shell, MigrationDirection::ToGateway).await
        }
        GatewayComamnds::NotifyAboutFromGatewayUpdate(args) => {
            gateway_common::notify_server(args, shell, MigrationDirection::FromGateway).await
        }
        GatewayComamnds::MigrateTokenBalances(args) => {
            migrate_token_balances::run(args, shell).await
        }
    }
}
