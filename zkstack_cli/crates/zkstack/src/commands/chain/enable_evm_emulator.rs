use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger};
use zkstack_cli_config::{ContractsGenesisConfig, ZkStackConfig, ZkStackConfigTrait};

use crate::{
    enable_evm_emulator::enable_evm_emulator,
    messages::{MSG_EVM_EMULATOR_ENABLED, MSG_EVM_EMULATOR_HASH_MISSING_ERR},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let genesis_config_path = chain_config.path_to_default_genesis_config();
    let default_genesis_config = ContractsGenesisConfig::read(shell, &genesis_config_path).await?;

    let has_evm_emulation_support = default_genesis_config.evm_emulator_hash()?.is_some();
    anyhow::ensure!(has_evm_emulation_support, MSG_EVM_EMULATOR_HASH_MISSING_ERR);

    let contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    enable_evm_emulator(
        shell,
        &chain_config.path_to_foundry_scripts(),
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
