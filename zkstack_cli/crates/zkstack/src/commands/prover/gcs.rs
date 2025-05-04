use xshell::{cmd, Shell};
use zkstack_cli_common::{
    check_prerequisites, cmd::Cmd, logger, spinner::Spinner, GCLOUD_PREREQUISITE,
};
use zkstack_cli_config::{ObjectStoreConfig, ObjectStoreMode};

use super::args::init::ProofStorageGCSCreateBucket;
use crate::{
    consts::PROVER_STORE_MAX_RETRIES,
    messages::{
        msg_bucket_created, MSG_CREATING_GCS_BUCKET_SPINNER, MSG_GETTING_GCP_PROJECTS_SPINNER,
    },
};

pub(crate) fn create_gcs_bucket(
    shell: &Shell,
    config: ProofStorageGCSCreateBucket,
) -> anyhow::Result<ObjectStoreConfig> {
    check_prerequisites(shell, &GCLOUD_PREREQUISITE, false);

    let bucket_name = config.bucket_name;
    let location = config.location;
    let project_id = config.project_id;
    let cmd = Cmd::new(cmd!(
        shell,
        "gcloud storage buckets create gs://{bucket_name} --location={location} --project={project_id}"
    ));
    let spinner = Spinner::new(MSG_CREATING_GCS_BUCKET_SPINNER);
    cmd.run()?;
    spinner.finish();

    logger::info(msg_bucket_created(&bucket_name));

    Ok(ObjectStoreConfig {
        mode: ObjectStoreMode::GCSWithCredentialFile {
            bucket_base_url: format!("gs://{}", bucket_name),
            gcs_credential_file_path: config.credentials_file,
        },
        max_retries: PROVER_STORE_MAX_RETRIES,
    })
}

pub(crate) fn get_project_ids(shell: &Shell) -> anyhow::Result<Vec<String>> {
    let spinner = Spinner::new(MSG_GETTING_GCP_PROJECTS_SPINNER);

    let mut cmd = Cmd::new(cmd!(
        shell,
        "gcloud projects list --format='value(projectId)'"
    ));
    let output = cmd.run_with_output()?;
    let project_ids: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| line.to_string())
        .collect();
    spinner.finish();
    Ok(project_ids)
}
