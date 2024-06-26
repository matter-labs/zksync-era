use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};
use zksync_config::{
    configs::{object_store::ObjectStoreMode, GeneralConfig},
    ObjectStoreConfig,
};

use super::{
    args::init::{ProofStorageGCSCreateBucket, ProverInitArgs, ProverInitArgsFinal},
    utils::get_link_to_prover,
};
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DOWNLOADING_SETUP_KEY_SPINNER, MSG_GENERAL_CONFIG_NOT_FOUND_ERR,
    MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR, MSG_PROVER_CONFIG_NOT_FOUND_ERR,
    MSG_PROVER_INITIALIZED,
};

const PROVER_STORE_MAX_RETRIES: u16 = 10;

pub(crate) async fn run(args: ProverInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let project_ids = get_project_ids(shell)?;
    let args = args.fill_values_with_prompt(project_ids);
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let object_store_config = match args {
        ProverInitArgsFinal::FileBacked(config) => ObjectStoreConfig {
            mode: ObjectStoreMode::FileBacked {
                file_backed_base_path: config.proof_store_dir,
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        },
        ProverInitArgsFinal::GCS(config) => ObjectStoreConfig {
            mode: ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url: config.bucket_base_url,
                gcs_credential_file_path: config.credentials_file,
            },
            max_retries: PROVER_STORE_MAX_RETRIES,
            local_mirror_path: None,
        },
        ProverInitArgsFinal::GCSCreateBucket(config) => create_gcs_bucket(shell, config)?,
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

    download_setup_key(shell, &general_config, &ecosystem_config)?;

    logger::outro(MSG_PROVER_INITIALIZED);
    Ok(())
}

fn create_gcs_bucket(
    shell: &Shell,
    config: ProofStorageGCSCreateBucket,
) -> anyhow::Result<ObjectStoreConfig> {
    let bucket_name = config.bucket_name;
    let location = config.location;
    let project_id = config.project_id;
    let mut cmd = Cmd::new(cmd!(
        shell,
        "gcloud storage buckets create gs://{bucket_name} --location={location} --project={project_id}"
    ));
    let spinner = Spinner::new("Creating GCS bucket...");
    cmd.run()?;
    spinner.finish();

    logger::info(format!(
        "Bucket created successfully with url: gs://{bucket_name}"
    ));

    Ok(ObjectStoreConfig {
        mode: ObjectStoreMode::GCSWithCredentialFile {
            bucket_base_url: format!("gs://{}", bucket_name),
            gcs_credential_file_path: config.credentials_file,
        },
        max_retries: PROVER_STORE_MAX_RETRIES,
        local_mirror_path: None,
    })
}

fn download_setup_key(
    shell: &Shell,
    general_config: &GeneralConfig,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_DOWNLOADING_SETUP_KEY_SPINNER);
    let compressor_config: zksync_config::configs::FriProofCompressorConfig = general_config
        .proof_compressor_config
        .as_ref()
        .expect(MSG_PROOF_COMPRESSOR_CONFIG_NOT_FOUND_ERR)
        .clone();
    let link_to_prover = get_link_to_prover(ecosystem_config);
    let path = link_to_prover.join(compressor_config.universal_setup_path);
    let url = compressor_config.universal_setup_download_url;

    let mut cmd = Cmd::new(cmd!(shell, "wget {url} -P {path}"));
    cmd.run()?;
    spinner.finish();
    Ok(())
}

fn get_project_ids(shell: &Shell) -> anyhow::Result<Vec<String>> {
    let mut cmd = Cmd::new(cmd!(
        shell,
        "gcloud projects list --format='value(projectId)'"
    ));
    let output = cmd.run_with_output()?;
    let project_ids: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| line.to_string())
        .collect();

    Ok(project_ids)
}
