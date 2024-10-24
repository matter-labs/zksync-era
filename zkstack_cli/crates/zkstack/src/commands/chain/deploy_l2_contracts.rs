use anyhow::Context as _;
use common::{
    contracts::{build_l2_contracts,Verifier},
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{
    forge_interface::{
        deploy_l2_contracts,
        script_params::DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DEPLOYING_L2_CONTRACT_SPINNER,
        MSG_L1_SECRETS_MUST_BE_PRESENTED,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

#[derive(clap::Parser, Debug)]
pub struct Command {
    #[clap(flatten)]
    pub args: ForgeScriptArgs,
    /// Whether to verify the contracts after deployment.
    #[clap(long)]
    pub l2_verify: bool,
}

impl Command {
    pub async fn run(self, shell: &Shell, contracts: Contracts) -> anyhow::Result<()> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config
            .load_current_chain()
            .context(MSG_CHAIN_NOT_INITIALIZED)?;
        let spinner = Spinner::new(MSG_DEPLOYING_L2_CONTRACT_SPINNER);
        let output = contracts.build_and_deploy(shell, self.args, &chain_config, &ecosystem_config).await
            .context("build_and_deploy()")?;
        let mut contracts_config = chain_config.get_contracts_config()?;
        contracts_config.set_l2_contracts(&output);
        contracts_config.save_with_base_path(shell, &chain_config.configs)?;
        if self.l2_verify {
            verify(shell, &ecosystem_config, &output).await.context("verify()")?;
        }
        spinner.finish();
        Ok(())
    }
}

#[derive(Default)]
pub struct Contracts {
    pub shared_bridge: bool,
    pub consensus_registry: bool,
    pub multicall3: bool,
    pub force_deploy_upgrader: bool,
}

impl Contracts {
    pub fn all() -> Self {
        Self {
            shared_bridge: true,
            consensus_registry: true,
            multicall3: true,
            force_deploy_upgrader: true,
        }
    } 

    pub async fn build_and_deploy(
        &self,
        shell: &Shell,
        args: ForgeScriptArgs,
        chain_config: &ChainConfig,
        ecosystem_config: &EcosystemConfig,
    ) -> anyhow::Result<deploy_l2_contracts::output::Output> {
        build_l2_contracts(shell, ecosystem_config.link_to_code.clone())?;
        let mut input = deploy_l2_contracts::input::Input::new(chain_config, ecosystem_config.era_chain_id)?;
        input.deploy_shared_bridge = self.shared_bridge;
        input.deploy_consensus_registry = self.consensus_registry;
        input.deploy_multicall3 = self.multicall3;
        input.deploy_force_deploy_upgrader = self.force_deploy_upgrader;
        call_forge(input, shell, chain_config, ecosystem_config, args).await
    }
}

pub async fn verify(shell: &Shell, ecosystem_config: &EcosystemConfig, output: &deploy_l2_contracts::output::Output) -> anyhow::Result<()> {
    // TODO: wait for contracts?
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let general_config = chain_config.get_general_config()?;
    let mut verifier_url : url::Url = general_config.contract_verifier.context("contract verifier config is missing")?.url.parse().context("failed to parse verifier_url")?;
    verifier_url.set_path("contract_verification");
    let v = Verifier {
        link_to_code: ecosystem_config.link_to_code.clone(),
        rpc_url: general_config.api_config.context(MSG_CHAIN_NOT_INITIALIZED)?.web3_json_rpc.http_url.parse().context("failed to parse rpc_url")?,
        verifier_url,
    };
    for spec in [
        &output.l2_shared_bridge_proxy,
        &output.l2_shared_bridge_implementation,
        &output.l2_force_deploy_upgrader,
        &output.l2_consensus_registry_proxy,
        &output.l2_consensus_registry_implementation,
        &output.l2_multicall3,
    ] {
        if let Some(spec) = spec.as_ref() {
            v.verify_l2_contract(shell, spec).await.context(spec.name.clone())?;
        }
    }
    Ok(())
}

async fn call_forge(
    input: deploy_l2_contracts::input::Input,
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<deploy_l2_contracts::output::Output> {
    let foundry_contracts_path = chain_config.path_to_foundry();
    let secrets = chain_config.get_secrets_config()?;
    input.save(
        shell,
        DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.input(&chain_config.link_to_code),
    )?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.script(), forge_args)
        .with_ffi()
        .with_rpc_url(
            secrets
                .l1
                .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
                .l1_rpc_url
                .expose_str()
                .to_string(),
        )
        .with_broadcast()
        .with_signature("deploy");

    forge = fill_forge_private_key(forge, Some(&ecosystem_config.get_wallets()?.governor))?;

    check_the_balance(&forge).await?;
    forge.run(shell)?;
    let out = &DEPLOY_L2_CONTRACTS_SCRIPT_PARAMS.output(&chain_config.link_to_code);
    Ok(deploy_l2_contracts::output::Output::read(shell, out)?)
}
