use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::{traits::SaveConfigWithBasePath, ZkStackConfig};

use super::{
    args::build_transactions::BuildTransactionsArgs,
    create_configs::create_initial_deployments_config,
};
use crate::{
    abi::IDEPLOYCTMABI_ABI,
    commands::ctm::commands::init_new_ctm::deploy_new_ctm,
    messages::{
        MSG_BUILDING_ECOSYSTEM, MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_ECOSYSTEM_TXN_OUTRO,
        MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR, MSG_WRITING_OUTPUT_FILES_SPINNER,
    },
};

const DEPLOY_TRANSACTIONS_FILE_DST: &str = "deploy-l1-txns.json";

fn get_deploy_transactions_file_src() -> String {
    use ethers::abi::Abi;

    let abi: &Abi = &IDEPLOYCTMABI_ABI;

    let run_selector = abi
        .functions()
        .find(|f| f.name == "run")
        .map(|f| ethers::utils::hex::encode(f.short_signature()))
        .expect("run selector not found");

    format!(
        "l1-contracts/broadcast/DeployCTM.s.sol/9/dry-run/{}-latest.json",
        run_selector
    )
}

const SCRIPT_CONFIG_FILE_SRC: &str = "l1-contracts/script-config/config-deploy-l1.toml";
const SCRIPT_CONFIG_FILE_DST: &str = "config-deploy-l1.toml";

pub async fn run(args: BuildTransactionsArgs, shell: &Shell) -> anyhow::Result<()> {
    let vm_option = args.common.vm_option();
    let args = args.fill_values_with_prompt()?;
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    logger::info(MSG_BUILDING_ECOSYSTEM);

    let spinner = Spinner::new(MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_new_ctm(
        shell,
        &args.forge_args,
        &ecosystem_config,
        &initial_deployment_config,
        &args.l1_rpc_url,
        Some(args.sender),
        false,
        false,
        args.bridgehub_address,
        vm_option,
        false,
    )
    .await?;

    contracts_config.save_with_base_path(shell, &args.out)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_WRITING_OUTPUT_FILES_SPINNER);
    shell
        .create_dir(&args.out)
        .context(MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        ecosystem_config
            .contracts_path_for_ctm(vm_option)
            .join(get_deploy_transactions_file_src()),
        args.out.join(DEPLOY_TRANSACTIONS_FILE_DST),
    )?;

    shell.copy_file(
        ecosystem_config
            .contracts_path_for_ctm(vm_option)
            .join(SCRIPT_CONFIG_FILE_SRC),
        args.out.join(SCRIPT_CONFIG_FILE_DST),
    )?;
    spinner.finish();

    logger::outro(MSG_ECOSYSTEM_TXN_OUTRO);

    Ok(())
}
