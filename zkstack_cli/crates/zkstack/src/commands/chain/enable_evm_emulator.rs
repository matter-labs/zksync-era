use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger};
use zkstack_cli_config::{EcosystemConfig, GenesisConfig, ZkStackConfig, GENESIS_FILE};

use crate::{
    enable_evm_emulator::enable_evm_emulator,
    messages::{MSG_EVM_EMULATOR_ENABLED, MSG_EVM_EMULATOR_HASH_MISSING_ERR},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let genesis_config_path =
        EcosystemConfig::default_configs_path(&chain_config.link_to_code).join(GENESIS_FILE);
    let default_genesis_config = GenesisConfig::read(shell, &genesis_config_path).await?;

    let has_evm_emulation_support = default_genesis_config.evm_emulator_hash()?.is_some();
    anyhow::ensure!(has_evm_emulation_support, MSG_EVM_EMULATOR_HASH_MISSING_ERR);

    let contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    enable_evm_emulator(
        shell,
        &chain_config.path_to_l1_foundry(),
        contracts.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts.l1.diamond_proxy_addr,
        &args,
        l1_rpc_url,
    )
    .await?;
    logger::success(MSG_EVM_EMULATOR_ENABLED);
    Ok(())
}
