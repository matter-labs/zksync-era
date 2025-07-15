use anyhow::Context as _;
use ethers::utils::hex::ToHexExt;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, spinner::Spinner};
use zkstack_cli_config::{traits::SaveConfigWithBasePath, ChainConfig, EcosystemConfig};

use crate::{
    commands::prover::args::deploy_proving_network::DeployProvingNetworkArgs,
    messages::MSG_DEPLOYING_PROVING_NETWORKS_SPINNER,
};

pub(crate) async fn run(shell: &Shell, args: DeployProvingNetworkArgs) -> anyhow::Result<()> {
    let config: EcosystemConfig = EcosystemConfig::from_file(shell)?;
    let chain_config = config.load_current_chain()?;
    deploy_proving_network(shell, &config, &chain_config, args).await
}

pub(crate) async fn deploy_proving_network(
    shell: &Shell,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    deploy_proving_network_args: DeployProvingNetworkArgs,
) -> anyhow::Result<()> {
    let rpc_url = chain_config.get_general_config().await?.l2_http_url()?;
    let args = deploy_proving_network_args.fill_values_with_prompt(&rpc_url);
    let dir_guard = shell.push_dir(config.path_to_proving_networks());

    let proving_networks_deploy_script_path = config.path_to_proving_networks_deploy_script();

    let wallets = config.get_wallets()?;

    if let Some(wallet) = wallets.deployer {
        let private_key = wallet.private_key_h256();
        if private_key.is_none() {
            return Err(anyhow::anyhow!(
                "Deployer wallet not found(for proving networks)"
            ));
        }
        let private_key = private_key.unwrap();
        shell.set_var("PRIVATE_KEY", private_key.encode_hex());
    } else {
        return Err(anyhow::anyhow!(
            "Deployer wallet not found(for proving networks)"
        ));
    }

    shell.set_var("RPC_URL", rpc_url);

    shell.set_var("FERMAH_ADDRESS", args.fermah_address);
    shell.set_var("LAGRANGE_ADDRESS", args.lagrange_address);
    shell.set_var("USDC_ADDRESS", args.usdc_address);
    shell.set_var(
        "PROOF_MANAGER_OWNER_ADDRESS",
        args.proof_manager_owner_address,
    );
    shell.set_var("PROXY_OWNER_ADDRESS", args.proxy_owner_address);

    let spinner = Spinner::new(MSG_DEPLOYING_PROVING_NETWORKS_SPINNER);

    let cmd = Cmd::new(cmd!(shell, "{proving_networks_deploy_script_path}"));

    let output = cmd
        .with_force_run()
        .run_with_output()
        .context("Failed to deploy proving networks")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to deploy proving networks: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let output = String::from_utf8(output.stdout).unwrap();

    let mut impl_addr = String::new();
    let mut proxy_addr = String::new();
    let mut proxy_admin_addr = String::new();

    for line in output.lines() {
        if line.contains("IMPLEMENTATION:") {
            impl_addr = line.split(":").nth(1).unwrap().to_string();
        }
        if line.contains("PROXY:") {
            proxy_addr = line.split(":").nth(1).unwrap().to_string();
        }
        if line.contains("PROXY_ADMIN:") {
            proxy_admin_addr = line.split(":").nth(1).unwrap().to_string();
        }
    }

    impl_addr = impl_addr.trim().to_string();
    proxy_addr = proxy_addr.trim().to_string();
    proxy_admin_addr = proxy_admin_addr.trim().to_string();

    logger::info(format!("Impl addr: {:?}", impl_addr).as_str());
    logger::info(format!("Proxy addr: {:?}", proxy_addr).as_str());
    logger::info(format!("Proxy admin addr: {:?}", proxy_admin_addr).as_str());

    drop(dir_guard);

    let mut contracts_config = config.get_contracts_config()?;

    contracts_config.set_eth_proof_manager_addresses(
        impl_addr.clone(),
        proxy_addr.clone(),
        proxy_admin_addr.clone(),
    )?;

    contracts_config.save_with_base_path(shell, &config.config)?;

    logger::info(format!(
        "Saving chain contracts config to {:?}",
        chain_config.configs
    ));

    let mut chain_contracts_config = chain_config.get_contracts_config()?;
    chain_contracts_config.set_eth_proof_manager_addresses(
        impl_addr,
        proxy_addr,
        proxy_admin_addr,
    )?;
    chain_contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    spinner.finish();

    Ok(())
}
