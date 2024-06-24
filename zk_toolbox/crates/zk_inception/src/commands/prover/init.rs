use common::logger;
use config::EcosystemConfig;
use xshell::Shell;
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};

use super::args::init::ProverInitArgs;
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_GENERAL_CONFIG_NOT_FOUND_ERR, MSG_PROVER_CONFIG_NOT_FOUND_ERR,
    MSG_PROVER_INITIALIZED,
};

const PROVER_STORE_MAX_RETRIES: u16 = 10;

pub(crate) async fn run(args: ProverInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let object_store_config = if args.proof_store_dir.is_some() {
        ObjectStoreConfig {
            mode: ObjectStoreMode::FileBacked {
                file_backed_base_path: args.proof_store_dir.unwrap(),
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        }
    } else {
        ObjectStoreConfig {
            mode: ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url: args.proof_store_gcs_config.bucket_base_url.unwrap(),
                gcs_credential_file_path: args.proof_store_gcs_config.credentials_file.unwrap(),
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        }
    };

    let chain_config = ecosystem_config
        .load_chain(Some(ecosystem_config.default_chain.clone()))
        .expect(MSG_CHAIN_NOT_FOUND_ERR);
    let mut general_config = chain_config
        .get_general_config()
        .expect(MSG_GENERAL_CONFIG_NOT_FOUND_ERR);
    let mut prover_config = general_config
        .prover_config
        .expect(MSG_PROVER_CONFIG_NOT_FOUND_ERR);
    prover_config.prover_object_store = Some(object_store_config.clone());
    general_config.prover_config = Some(prover_config);
    chain_config.save_general_config(&general_config)?;

    logger::outro(MSG_PROVER_INITIALIZED);
    Ok(())
}
