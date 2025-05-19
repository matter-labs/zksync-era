use std::path::{Path, PathBuf};

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::{get_link_to_prover, EcosystemConfig, GeneralConfigPatch};

use super::args::compressor_keys::CompressorKeysArgs;
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER, MSG_SETUP_KEY_PATH_ERROR,
};

pub(crate) async fn run(shell: &Shell, args: CompressorKeysArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let mut general_config = chain_config.get_general_config().await?.patched();

    let default_path = get_default_compressor_keys_path(&ecosystem_config)?;
    let args = args.fill_values_with_prompt(&default_path);

    let path = args.path.context(MSG_SETUP_KEY_PATH_ERROR)?;

    download_compressor_key(shell, &mut general_config, &path)?;

    general_config.save().await?;
    Ok(())
}

pub(crate) fn download_compressor_key(
    shell: &Shell,
    general_config: &mut GeneralConfigPatch,
    path: &Path,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER);
    general_config.set_proof_compressor_setup_path(path)?;
    let url = general_config.proof_compressor_setup_download_url()?;
    logger::info(format!("Downloading setup key by URL: {url}"));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let response = client.get(url).send()?.bytes()?;
    shell.write_file(path, &response)?;

    spinner.finish();
    Ok(())
}

pub fn get_default_compressor_keys_path(
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<PathBuf> {
    let link_to_prover = get_link_to_prover(ecosystem_config);
    Ok(link_to_prover.join("keys/setup/setup_compact.key"))
}
