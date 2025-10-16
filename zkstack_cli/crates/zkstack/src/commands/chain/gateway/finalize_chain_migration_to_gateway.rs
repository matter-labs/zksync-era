use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{config::global_config, forge::ForgeScriptArgs, logger};
use zkstack_cli_config::{ChainConfig, EcosystemConfig, ZkStackConfig, ZkStackConfigTrait};

use super::{
    migrate_to_gateway::get_migrate_to_gateway_params,
    migrate_to_gateway_calldata::get_migrate_to_gateway_data,
};
use crate::{
    admin_functions::AdminScriptMode,
    commands::chain::{
        gateway::migrate_to_gateway_calldata::check_permanent_rollup_and_set_da_validator_via_gateway,
        init::send_priority_txs,
    },
    messages::{
        msg_initializing_chain, MSG_CHAIN_INITIALIZED, MSG_CHAIN_NOT_INITIALIZED,
        MSG_DEPLOY_PAYMASTER_PROMPT, MSG_SELECTED_CONFIG,
    },
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct FinalizeChainMigrationToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_paymaster: Option<bool>,
}

impl FinalizeChainMigrationToGatewayArgs {
    pub fn fill_values_with_prompt(self) -> FinalizeChainMigrationToGatewayArgsFinal {
        let deploy_paymaster = self.deploy_paymaster.unwrap_or_else(|| {
            zkstack_cli_common::PromptConfirm::new(MSG_DEPLOY_PAYMASTER_PROMPT)
                .default(true)
                .ask()
        });

        FinalizeChainMigrationToGatewayArgsFinal {
            forge_args: self.forge_args,
            gateway_chain_name: self.gateway_chain_name,
            deploy_paymaster,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinalizeChainMigrationToGatewayArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub gateway_chain_name: String,
    pub deploy_paymaster: bool,
}

pub async fn run(args: FinalizeChainMigrationToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;

    let args = args.fill_values_with_prompt();

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));
    logger::info(msg_initializing_chain(""));

    run_inner(
        &args,
        shell,
        &ecosystem_config,
        &chain_config,
        &gateway_chain_config,
    )
    .await?;

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}

pub async fn run_inner(
    args: &FinalizeChainMigrationToGatewayArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    gateway_chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    let l1_rpc_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let mut contracts_config = chain_config.get_contracts_config()?;

    // Sends the priority txs that were skipped when the chain was initialized
    send_priority_txs(
        shell,
        chain_config,
        ecosystem_config,
        &mut contracts_config,
        &args.forge_args,
        l1_rpc_url,
        args.deploy_paymaster,
    )
    .await?;

    // Set the DA validator pair on the Gateway
    let params = get_migrate_to_gateway_params(chain_config, gateway_chain_config).await?;
    let data = get_migrate_to_gateway_data(&params, true).await?;

    let (_, l2_da_validator) = data.l1_zk_chain.get_da_validator_pair().await?;
    check_permanent_rollup_and_set_da_validator_via_gateway(
        shell,
        &args.forge_args,
        &chain_config.path_to_foundry_scripts(),
        &data,
        &params,
        l2_da_validator,
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
    )
    .await?;

    Ok(())
}
