use common::{check_prover_prequisites, cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};
use zksync_config::{
    configs::{object_store::ObjectStoreMode, GeneralConfig},
    ObjectStoreConfig,
};

use super::{
    args::init::{ProofStorageConfig, ProverInitArgs},
    gcs::create_gcs_bucket,
    utils::get_link_to_prover,
};
use crate::{
    consts::{BELLMAN_CUDA_DIR, PROVER_STORE_MAX_RETRIES},
    messages::{
        MSG_BUILDING_BELLMAN_CUDA_SPINNER, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_CLONING_BELLMAN_CUDA_SPINNER, MSG_DOWNLOADING_SETUP_KEY_SPINNER,
        MSG_GENERAL_CONFIG_NOT_FOUND_ERR, MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR,
        MSG_PROVER_CONFIG_NOT_FOUND_ERR, MSG_PROVER_INITIALIZED, MSG_SETUP_KEY_PATH_ERROR,
    },
};

pub(crate) async fn run(args: ProverInitArgs, shell: &Shell) -> anyhow::Result<()> {
    check_prover_prequisites(shell);
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(Some(ecosystem_config.default_chain.clone()))
        .expect(MSG_CHAIN_NOT_FOUND_ERR);
    let mut general_config = chain_config
        .get_zksync_general_config()
        .expect(MSG_GENERAL_CONFIG_NOT_FOUND_ERR);

    let setup_key_path = get_default_setup_key_path(&ecosystem_config)?;

    let args = args.fill_values_with_prompt(shell, &setup_key_path)?;

    let proof_object_store_config = get_object_store_config(shell, Some(args.proof_store))?;
    let public_object_store_config = get_object_store_config(shell, args.public_store)?;

    if args.setup_key_config.download_key {
        download_setup_key(
            shell,
            &general_config,
            &args.setup_key_config.setup_key_path,
        )?;
    }

    let mut prover_config = general_config
        .prover_config
        .expect(MSG_PROVER_CONFIG_NOT_FOUND_ERR);
    prover_config
        .prover_object_store
        .clone_from(&proof_object_store_config);
    if let Some(public_object_store_config) = public_object_store_config {
        prover_config.shall_save_to_public_bucket = true;
        prover_config.public_object_store = Some(public_object_store_config);
    } else {
        prover_config.shall_save_to_public_bucket = false;
    }
    general_config.prover_config = Some(prover_config);

    let mut proof_compressor_config = general_config
        .proof_compressor_config
        .expect(MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR);
    proof_compressor_config.universal_setup_path = args.setup_key_config.setup_key_path;
    general_config.proof_compressor_config = Some(proof_compressor_config);

    chain_config.save_zksync_general_config(&general_config)?;

    init_bellman_cuda(shell)?;

    logger::outro(MSG_PROVER_INITIALIZED);
    Ok(())
}

fn download_setup_key(
    shell: &Shell,
    general_config: &GeneralConfig,
    path: &str,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_DOWNLOADING_SETUP_KEY_SPINNER);
    let compressor_config: zksync_config::configs::FriProofCompressorConfig = general_config
        .proof_compressor_config
        .as_ref()
        .expect(MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR)
        .clone();
    let url = compressor_config.universal_setup_download_url;
    let parent = std::path::Path::new(path)
        .parent()
        .expect(MSG_SETUP_KEY_PATH_ERROR);

    Cmd::new(cmd!(shell, "wget {url} -P {parent}")).run()?;

    spinner.finish();
    Ok(())
}

fn get_default_setup_key_path(ecosystem_config: &EcosystemConfig) -> anyhow::Result<String> {
    let link_to_prover = get_link_to_prover(ecosystem_config);
    let path = link_to_prover.join("keys/setup/setup_2^24.key");
    let string = path.to_str().unwrap();

    Ok(String::from(string))
}

fn get_object_store_config(
    shell: &Shell,
    config: Option<ProofStorageConfig>,
) -> anyhow::Result<Option<ObjectStoreConfig>> {
    let object_store = match config {
        Some(ProofStorageConfig::FileBacked(config)) => Some(ObjectStoreConfig {
            mode: ObjectStoreMode::FileBacked {
                file_backed_base_path: config.proof_store_dir,
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        }),
        Some(ProofStorageConfig::GCS(config)) => Some(ObjectStoreConfig {
            mode: ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url: config.bucket_base_url,
                gcs_credential_file_path: config.credentials_file,
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        }),
        Some(ProofStorageConfig::GCSCreateBucket(config)) => {
            Some(create_gcs_bucket(shell, config)?)
        }
        None => None,
    };

    Ok(object_store)
}

fn init_bellman_cuda(shell: &Shell) -> anyhow::Result<()> {
    if shell.path_exists(BELLMAN_CUDA_DIR) {
        return Ok(());
    }

    let spinner = Spinner::new(MSG_CLONING_BELLMAN_CUDA_SPINNER);
    Cmd::new(cmd!(
        shell,
        "git clone https://github.com/matter-labs/era-bellman-cuda"
    ))
    .run()?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_BELLMAN_CUDA_SPINNER);
    Cmd::new(cmd!(
        shell,
        "cmake -Bera-bellman-cuda/build -Sera-bellman-cuda/ -DCMAKE_BUILD_TYPE=Release"
    ))
    .run()?;
    Cmd::new(cmd!(shell, "cmake --build era-bellman-cuda/build")).run()?;
    spinner.finish();
    Ok(())
}
