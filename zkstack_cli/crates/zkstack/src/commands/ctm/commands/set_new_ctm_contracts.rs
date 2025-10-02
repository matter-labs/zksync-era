use std::path::PathBuf;

use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{traits::SaveConfigWithBasePath, EcosystemConfig, ZkStackConfig};

use crate::{
    commands::ctm::args::SetNewCTMArgs, messages::MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
};

pub(crate) fn run(args: SetNewCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let config = ZkStackConfig::ecosystem(shell)?;
    let args = args.fill_values_with_prompt()?;
    set_new_ctm_contracts(
        shell,
        config,
        args.contracts_src_path,
        args.default_configs_src_path,
        args.zksync_os,
    )?;
    Ok(())
}

pub fn set_new_ctm_contracts(
    shell: &Shell,
    mut ecosystem_config: EcosystemConfig,
    contracts_path: PathBuf,
    default_configs_path: PathBuf,
    zksync_os: bool,
) -> anyhow::Result<EcosystemConfig> {
    if !contracts_path.exists() || !contracts_path.is_dir() {
        return Err(anyhow::anyhow!(MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR));
    }

    if !default_configs_path.exists() || !default_configs_path.is_dir() {
        return Err(anyhow::anyhow!(MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR));
    }

    logger::info(format!(
        "Using contracts source path: {} and default_configs_path: {}",
        contracts_path.display(),
        default_configs_path.display()
    ));
    ecosystem_config.set_sources_path(contracts_path, default_configs_path, zksync_os);
    ecosystem_config.save_with_base_path(shell, ".")?;
    Ok(ecosystem_config)
}
