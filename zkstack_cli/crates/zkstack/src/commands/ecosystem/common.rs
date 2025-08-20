use xshell::Shell;
use zkstack_cli_common::forge::{Forge, ForgeScriptArgs};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{DeployL1Config, GenesisInput, InitialDeploymentConfig},
            output::DeployL1Output,
        },
        script_params::DEPLOY_ECOSYSTEM_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig},
    ContractsConfig, EcosystemConfig, GenesisConfig, GENESIS_FILE,
};
use zkstack_cli_types::{L1Network, ProverMode};

use crate::utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner};

#[allow(clippy::too_many_arguments)]
pub async fn deploy_l1(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
    support_l2_legacy_shared_bridge_test: bool,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.input(&config.path_to_l1_foundry());
    let genesis_config_path = config.get_default_configs_path().join(GENESIS_FILE);
    let default_genesis_config = GenesisConfig::read(shell, &genesis_config_path).await?;
    let default_genesis_input = GenesisInput::new(&default_genesis_config)?;

    let wallets_config = config.get_wallets()?;
    // For deploying ecosystem we only need genesis batch params
    let deploy_config = DeployL1Config::new(
        &default_genesis_input,
        &wallets_config,
        initial_deployment_config,
        config.era_chain_id,
        config.prover_version == ProverMode::NoProofs,
        config.l1_network,
        support_l2_legacy_shared_bridge_test,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url.to_string());

    if config.l1_network == L1Network::Localhost {
        // It's a kludge for reth, just because it doesn't behave properly with large amount of txs
        forge = forge.with_slow();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(
            forge,
            wallets_config.deployer.as_ref(),
            WalletOwner::Deployer,
        )?;
    }

    if broadcast {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let script_output = DeployL1Output::read(
        shell,
        DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.output(&config.path_to_l1_foundry()),
    )?;
    let mut contracts_config = ContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);

    Ok(contracts_config)
}
