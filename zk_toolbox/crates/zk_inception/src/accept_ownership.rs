use common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{
    forge_interface::{
        accept_ownership::AcceptOwnershipInput, script_params::ACCEPT_GOVERNANCE_SCRIPT_PARAMS,
    },
    traits::SaveConfig,
    EcosystemConfig,
};
use ethers::types::{Address, H256};
use xshell::Shell;

use crate::{
    messages::MSG_ACCEPTING_GOVERNANCE_SPINNER,
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn accept_admin(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: Option<H256>,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let foundry_contracts_path = ecosystem_config.path_to_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(
            &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_signature("acceptAdmin()");
    accept_ownership(
        shell,
        ecosystem_config,
        governor_contract,
        governor,
        target_address,
        forge,
    )
    .await
}

pub async fn accept_owner(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: Option<H256>,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let foundry_contracts_path = ecosystem_config.path_to_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(
            &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_signature("acceptOwner()");
    accept_ownership(
        shell,
        ecosystem_config,
        governor_contract,
        governor,
        target_address,
        forge,
    )
    .await
}

async fn accept_ownership(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: Option<H256>,
    target_address: Address,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    let input = AcceptOwnershipInput {
        target_addr: target_address,
        governor: governor_contract,
    };
    input.save(
        shell,
        ACCEPT_GOVERNANCE_SCRIPT_PARAMS.input(&ecosystem_config.link_to_code),
    )?;

    forge = fill_forge_private_key(forge, governor)?;

    check_the_balance(&forge).await?;
    let spinner = Spinner::new(MSG_ACCEPTING_GOVERNANCE_SPINNER);
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
