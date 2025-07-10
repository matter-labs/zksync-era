use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::build_tee_contracts,
    forge::{Forge, ForgeScriptArgs},
    logger,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_tee::{input::DeployTeeInput, output::DeployTeeOutput},
        script_params::DEPLOY_TEE_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig},
    ContractsConfig, EcosystemConfig,
};

use crate::utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner};

/// Deploy TEE DCAP attestation contracts as part of the ecosystem initialization
pub async fn deploy_tee_contracts(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: &str,
) -> anyhow::Result<()> {
    // Build the TEE contracts first
    build_tee_contracts(shell.clone(), ecosystem_config.link_to_code.clone())?;

    // Deploy the contracts
    call_forge_tee_deploy(shell, ecosystem_config, forge_args, l1_rpc_url).await?;

    // Read the deployment output and update the config
    update_config_with_tee_contracts(shell, ecosystem_config, contracts_config)?;

    // Log success message
    logger::info("TEE DCAP attestation contracts deployed successfully");

    Ok(())
}

/// Call the forge script to deploy TEE DCAP contracts
async fn call_forge_tee_deploy(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: &str,
) -> anyhow::Result<()> {
    let tee_contracts_path = ecosystem_config
        .link_to_code
        .join("contracts/tee-contracts");

    let wallets = ecosystem_config.get_wallets()?;
    let tee_dcap_operator_address = wallets.tee_dcap_operator.as_ref().unwrap().address;
    let input = DeployTeeInput::new(tee_dcap_operator_address);
    input.save(shell, DEPLOY_TEE_SCRIPT_PARAMS.input(&tee_contracts_path))?;

    // Create forge instance
    let mut forge = Forge::new(&tee_contracts_path)
        .script(&DEPLOY_TEE_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url.to_string())
        .with_broadcast();

    // Set the deployer private key
    forge = fill_forge_private_key(
        forge,
        // FIXME: TEE - is governor or deployer the correct owner?
        // ecosystem_config.get_wallets()?.deployer.as_ref(),
        Some(&ecosystem_config.get_wallets()?.governor),
        WalletOwner::TEE,
    )?;

    // Check balance and run forge script
    check_the_balance(&forge).await?;
    logger::info("Running forge script...");
    forge.run(shell)?;

    Ok(())
}

/// Read deployment output and update contract configuration
fn update_config_with_tee_contracts(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &mut ContractsConfig,
) -> anyhow::Result<()> {
    // Read the deployment output TOML
    let tee_contracts_path = ecosystem_config
        .link_to_code
        .join("contracts/tee-contracts");
    let output_path = DEPLOY_TEE_SCRIPT_PARAMS.output(&tee_contracts_path);

    // Read the deployment output using the ReadConfig trait
    let deployment = DeployTeeOutput::read(shell, output_path)
        .context("Failed to read TEE deployment output")?;

    // Update contracts config
    contracts_config.set_tee_dcap_attestation_addr(deployment.tee_dcap_attestation_addr)?;

    logger::info(format!(
        "TEE DCAP Attestation contract deployed at: {}",
        deployment.tee_dcap_attestation_addr
    ));

    Ok(())
}
