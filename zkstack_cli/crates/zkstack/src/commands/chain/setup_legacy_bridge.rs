use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeRunner, ForgeScriptArgs},
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        script_params::SETUP_LEGACY_BRIDGE, setup_legacy_bridge::SetupLegacyBridgeInput,
    },
    traits::SaveConfig,
    ChainConfig, ContractsConfig, EcosystemConfig, ZkStackConfigTrait,
};

use crate::{
    messages::MSG_DEPLOYING_PAYMASTER,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub async fn setup_legacy_bridge(
    shell: &Shell,
    runner: &mut ForgeRunner,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let input = SetupLegacyBridgeInput {
        bridgehub: contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        diamond_proxy: contracts_config.l1.diamond_proxy_addr,
        shared_bridge_proxy: contracts_config.bridges.shared.l1_address,
        transparent_proxy_admin: contracts_config
            .ecosystem_contracts
            .transparent_proxy_admin_addr,
        l1_nullifier_proxy: contracts_config
            .bridges
            .l1_nullifier_addr
            .context("`l1_nullifier` missing")?,
        l1_native_token_vault: contracts_config
            .ecosystem_contracts
            .native_token_vault_addr
            .context("`native_token_vault` missing")?,
        erc20bridge_proxy: contracts_config.bridges.erc20.l1_address,
        token_weth_address: Default::default(),
        chain_id: chain_config.chain_id,
        l2shared_bridge_address: contracts_config
            .bridges
            .shared
            .l2_address
            .expect("Not fully initialized"),
        create2factory_salt: contracts_config.create2_factory_salt,
        create2factory_addr: contracts_config.create2_factory_addr,
    };
    let foundry_contracts_path = chain_config.path_to_foundry_scripts();
    input.save(
        shell,
        SETUP_LEGACY_BRIDGE.input(&chain_config.path_to_foundry_scripts()),
    )?;
    let secrets = chain_config.get_secrets_config().await?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&SETUP_LEGACY_BRIDGE.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(secrets.l1_rpc_url()?)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        Some(&ecosystem_config.get_wallets()?.governor),
        WalletOwner::Governor,
    )?;

    let spinner = Spinner::new(MSG_DEPLOYING_PAYMASTER);
    check_the_balance(&forge).await?;
    runner.run(shell, forge)?;
    spinner.finish();

    Ok(())
}
