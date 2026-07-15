use std::path::PathBuf;

use xshell::Shell;
use zkstack_cli_common::{git, logger};
use zkstack_cli_config::{traits::SaveConfigWithBasePath, EcosystemConfig, ZkStackConfig};
use zkstack_cli_types::VMOption;

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
        args.vm_option,
    )?;
    Ok(())
}

pub fn set_new_ctm_contracts(
    shell: &Shell,
    mut ecosystem_config: EcosystemConfig,
    contracts_path: PathBuf,
    default_configs_path: PathBuf,
    vm_option: VMOption,
) -> anyhow::Result<EcosystemConfig> {
    logger::info(format!(
        "Setting new CTM contracts source path to: {}",
        contracts_path.display()
    ));
    if !contracts_path.exists() || !contracts_path.is_dir() {
        return Err(anyhow::anyhow!(MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR));
    }

    if !default_configs_path.exists() || !default_configs_path.is_dir() {
        return Err(anyhow::anyhow!(MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR));
    }
    // Update submodules to make sure we have the latest code.
    git::submodule_update(shell, &contracts_path)?;
    logger::info(format!(
        "Using contracts source path: {} and default_configs_path: {}",
        contracts_path.display(),
        default_configs_path.display()
    ));
    ecosystem_config.set_sources_path(contracts_path, default_configs_path, vm_option);
    ecosystem_config.save_with_base_path(shell, ".")?;
    Ok(ecosystem_config)
}
