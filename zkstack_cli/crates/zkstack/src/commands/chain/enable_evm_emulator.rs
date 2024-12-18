use anyhow::Context;
use common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::Shell;

use crate::{
    enable_evm_emulator::enable_evm_emulator,
    messages::{
        MSG_ENABLING_EVM_EMULATOR, MSG_CHAIN_NOT_INITIALIZED, MSG_EVM_EMULATOR_ENABLED,
        MSG_L1_SECRETS_MUST_BE_PRESENTED,
    },
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config()?;
    let l1_rpc_url = secrets
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();

    enable_evm_emulator(
        shell,
        &ecosystem_config,
        contracts.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts.l1.diamond_proxy_addr,
        &args,
        l1_rpc_url.clone(),
    )
    .await?;
    logger::success(MSG_EVM_EMULATOR_ENABLED);
    Ok(())
}
