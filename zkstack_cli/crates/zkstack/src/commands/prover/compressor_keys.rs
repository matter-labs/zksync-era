use anyhow::Context;
use common::{logger, spinner::Spinner};
use config::{get_link_to_prover, EcosystemConfig, GeneralConfig};
use xshell::Shell;

use super::args::compressor_keys::{CompressorKeysArgs, CompressorType};
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER,
    MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR, MSG_SETUP_KEY_PATH_ERROR,
};

pub(crate) async fn run(shell: &Shell, args: CompressorKeysArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let mut general_config = chain_config.get_general_config()?;

    let default_plonk_path = get_default_plonk_compressor_keys_path(&ecosystem_config)?;
    let default_fflonk_path = get_default_fflonk_compressor_keys_path(&ecosystem_config)?;
    let args = args.fill_values_with_prompt(&default_plonk_path, &default_fflonk_path);

    match args.compressor_type {
        CompressorType::Fflonk => {
            let path = args.clone().fflonk_path.context(MSG_SETUP_KEY_PATH_ERROR)?;

            download_compressor_key(shell, &mut general_config, CompressorType::Fflonk, &path)?;
        }
        CompressorType::Plonk => {
            let path = args.plonk_path.context(MSG_SETUP_KEY_PATH_ERROR)?;

            download_compressor_key(shell, &mut general_config, CompressorType::Plonk, &path)?;
        }
        CompressorType::All => {
            let plonk_path = args.clone().plonk_path.context(MSG_SETUP_KEY_PATH_ERROR)?;
            let fflonk_path = args.clone().fflonk_path.context(MSG_SETUP_KEY_PATH_ERROR)?;

            download_compressor_key(
                shell,
                &mut general_config,
                CompressorType::Fflonk,
                &fflonk_path,
            )?;
            download_compressor_key(
                shell,
                &mut general_config,
                CompressorType::Plonk,
                &plonk_path,
            )?;
        }
    }

    chain_config.save_general_config(&general_config)?;

    Ok(())
}

pub(crate) fn download_compressor_key(
    shell: &Shell,
    general_config: &mut GeneralConfig,
    r#type: CompressorType,
    path: &str,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER);
    let mut compressor_config: zksync_config::configs::FriProofCompressorConfig = general_config
        .proof_compressor_config
        .as_ref()
        .expect(MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR)
        .clone();

    let url = match r#type {
        CompressorType::Fflonk => {
            compressor_config.universal_fflonk_setup_path = path.to_string();
            general_config.proof_compressor_config = Some(compressor_config.clone());
            compressor_config.universal_fflonk_setup_download_url
        }
        CompressorType::Plonk => {
            compressor_config.universal_setup_path = path.to_string();
            general_config.proof_compressor_config = Some(compressor_config.clone());
            compressor_config.universal_setup_download_url
        }
        _ => unreachable!("Invalid compressor type"),
    };

    let path = std::path::Path::new(path);

    logger::info(format!("Downloading setup key by URL: {}", url));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let response = client.get(url).send()?.bytes()?;
    shell.write_file(path, &response)?;

    spinner.finish();
    Ok(())
}

pub fn get_default_plonk_compressor_keys_path(
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<String> {
    let link_to_prover = get_link_to_prover(ecosystem_config);
    let path = link_to_prover.join("keys/setup/setup_2^24.key");
    let string = path.to_str().unwrap();

    Ok(String::from(string))
}

pub fn get_default_fflonk_compressor_keys_path(
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<String> {
    let link_to_prover = get_link_to_prover(ecosystem_config);
    let path = link_to_prover.join("keys/setup/setup_fflonk_compact.key");
    let string = path.to_str().unwrap();

    Ok(String::from(string))
}
