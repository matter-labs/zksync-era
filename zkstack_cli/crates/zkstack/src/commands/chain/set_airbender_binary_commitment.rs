use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};
use zksync_basic_types::H256;

use crate::{
    admin_functions::{set_airbender_binary_commitment, AdminScriptMode},
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_SETTING_AIRBENDER_BINARY_COMMITMENT_SPINNER},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct SetAirbenderBinaryCommitmentArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    /// Commitment to the airbender verifier guest binary, in the byte order the SNARK wrapper hashes
    /// it (the guest's `[u32; 8]` commitment serialized as 32 little-endian bytes).
    pub airbender_binary_commitment: H256,
}

pub async fn run(args: SetAirbenderBinaryCommitmentArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell).context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_rpc_url = chain_config
        .get_secrets_config()
        .await?
        .l1_rpc_url()?
        .to_string();

    let spinner = Spinner::new(MSG_SETTING_AIRBENDER_BINARY_COMMITMENT_SPINNER);
    set_airbender_binary_commitment(
        shell,
        &args.forge_args,
        &chain_config.path_to_foundry_scripts(),
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
        chain_config.chain_id.as_u64(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        args.airbender_binary_commitment,
        l1_rpc_url,
    )
    .await?;
    spinner.finish();

    logger::note(
        "Airbender binary commitment set to:",
        format!("{:?}", args.airbender_binary_commitment),
    );

    Ok(())
}
