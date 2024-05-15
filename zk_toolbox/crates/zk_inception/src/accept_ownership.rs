use common::{
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use ethers::{abi::Address, types::H256};
use xshell::Shell;

use crate::{
    configs::{
        forge_interface::accept_ownership::AcceptOwnershipInput, EcosystemConfig, SaveConfig,
    },
    consts::ACCEPT_GOVERNANCE,
    forge_utils::fill_forge_private_key,
};

pub fn accept_ownership(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: Option<H256>,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
) -> anyhow::Result<()> {
    let input = AcceptOwnershipInput {
        target_addr: target_address,
        governor: governor_contract,
    };
    let foundry_contracts_path = ecosystem_config.path_to_foundry();
    input.save(
        shell,
        ACCEPT_GOVERNANCE.input(&ecosystem_config.link_to_code),
    )?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&ACCEPT_GOVERNANCE.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(ecosystem_config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(forge, governor)?;

    let spinner = Spinner::new("Accepting governance");
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
