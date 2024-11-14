use anyhow::Context;
use common::{ethereum, spinner::Spinner};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use crate::messages::{
    MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED, MSG_REGISTERING_CHAIN_SPINNER,
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    run_propose_chain_registration(&chain_config).await
}

pub async fn run_propose_chain_registration(chain_config: &ChainConfig) -> anyhow::Result<()> {
    let contracts = chain_config.get_contracts_config()?;
    let wallets = chain_config.get_wallets_config()?;
    let genesis = chain_config.get_genesis_config()?;
    let secrets = chain_config.get_secrets_config()?;
    let l1_rpc_url = secrets
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    ethereum::propose_registration(
        contracts.ecosystem_contracts.chain_registrar,
        wallets.governor.clone(),
        l1_rpc_url,
        genesis.l1_chain_id.0,
        genesis.l2_chain_id.as_u64(),
        wallets.blob_operator.address,
        wallets.operator.address,
        wallets.governor.address,
        chain_config.base_token.address,
        wallets.token_multiplier_setter.map(|a| a.address),
        chain_config.base_token.nominator,
        chain_config.base_token.denominator,
        genesis.l1_batch_commit_data_generator_mode,
    )
    .await?;
    spinner.finish();
    Ok(())
}
