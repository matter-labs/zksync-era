use anyhow::Context;
use common::{
    check_prerequisites, cmd::Cmd, config::global_config, spinner::Spinner, WGET_PREREQUISITE,
};
use config::{get_link_to_prover, EcosystemConfig, GeneralConfig};
use xshell::{cmd, Shell};

use super::args::compressor_keys::CompressorKeysArgs;
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER,
    MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR, MSG_SETUP_KEY_PATH_ERROR,
};

pub(crate) async fn run(shell: &Shell, args: CompressorKeysArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let mut general_config = chain_config.get_general_config()?;

    let default_path = get_default_compressor_keys_path(&ecosystem_config)?;
    let args = args.fill_values_with_prompt(&default_path);

    download_compressor_key(
        shell,
        &mut general_config,
        &args.path.context(MSG_SETUP_KEY_PATH_ERROR)?,
    )?;

    chain_config.save_general_config(&general_config)?;

    Ok(())
}

pub(crate) fn download_compressor_key(
    shell: &Shell,
    general_config: &mut GeneralConfig,
    path: &str,
) -> anyhow::Result<()> {
    check_prerequisites(shell, &WGET_PREREQUISITE, false);
    let spinner = Spinner::new(MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER);
    let mut compressor_config: zksync_config::configs::FriProofCompressorConfig = general_config
        .proof_compressor_config
        .as_ref()
        .expect(MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR)
        .clone();
    compressor_config.universal_setup_path = path.to_string();
    general_config.proof_compressor_config = Some(compressor_config.clone());

    let url = compressor_config.universal_setup_download_url;
    let path = std::path::Path::new(path);
    let parent = path.parent().expect(MSG_SETUP_KEY_PATH_ERROR);
    let file_name = path.file_name().expect(MSG_SETUP_KEY_PATH_ERROR);

    Cmd::new(cmd!(shell, "wget {url} -P {parent}")).run()?;

    if file_name != "setup_2^24.key" {
        Cmd::new(cmd!(shell, "mv {parent}/setup_2^24.key {path}")).run()?;
    }

    spinner.finish();
    Ok(())
}

pub fn get_default_compressor_keys_path(
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<String> {
    let link_to_prover = get_link_to_prover(ecosystem_config);
    let path = link_to_prover.join("keys/setup/setup_2^24.key");
    let string = path.to_str().unwrap();

    Ok(String::from(string))
}
