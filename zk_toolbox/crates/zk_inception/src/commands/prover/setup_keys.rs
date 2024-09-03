use anyhow::Ok;
use common::{
    check_prerequisites, cmd::Cmd, logger, spinner::Spinner, GCLOUD_PREREQUISITES,
    GPU_PREREQUISITES,
};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::utils::get_link_to_prover;
use crate::{
    commands::prover::args::setup_keys::{Mode, Region, SetupKeysArgs},
    messages::{MSG_GENERATING_SK_SPINNER, MSG_SK_GENERATED},
};

pub(crate) async fn run(args: SetupKeysArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    if args.mode == Mode::Generate {
        check_prerequisites(shell, &GPU_PREREQUISITES, false);
        let link_to_prover = get_link_to_prover(&ecosystem_config);
        shell.change_dir(&link_to_prover);

        let spinner = Spinner::new(MSG_GENERATING_SK_SPINNER);
        let cmd = Cmd::new(cmd!(
            shell,
            "cargo run --features gpu --release --bin key_generator -- 
            generate-sk-gpu all --recompute-if-missing
            --setup-path=data/keys
            --path={link_to_prover}/data/keys"
        ));
        cmd.run()?;
        spinner.finish();
        logger::outro(MSG_SK_GENERATED);
    } else {
        check_prerequisites(shell, &GCLOUD_PREREQUISITES, false);

        let link_to_setup_keys = get_link_to_prover(&ecosystem_config).join("data/keys");
        let path_to_keys_buckets =
            get_link_to_prover(&ecosystem_config).join("setup-data-gpu-keys.json");

        let region = args.region.expect("Region is not provided");

        let file = shell
            .read_file(path_to_keys_buckets)
            .expect("Could not find commitments file in zksync-era");
        let json: serde_json::Value =
            serde_json::from_str(&file).expect("Could not parse commitments.json");

        let bucket = &match region {
            Region::Us => json
                .get("us")
                .expect("Could not find link to US bucket")
                .to_string(),
            Region::Europe => json
                .get("europe")
                .expect("Could not find link to Europe bucket")
                .to_string(),
            Region::Asia => json
                .get("asia")
                .expect("Could not find link to Asia bucket")
                .to_string(),
        };

        let len = bucket.len() - 2usize;
        let bucket = &bucket[1..len];

        let spinner = Spinner::new(&format!(
            "Downloading keys from bucket: {} to {:?}",
            bucket, link_to_setup_keys
        ));

        let cmd = Cmd::new(cmd!(
            shell,
            "gsutil -m rsync -r {bucket} {link_to_setup_keys}"
        ));
        cmd.run()?;
        spinner.finish();
        logger::outro("Keys are downloaded");
    }

    Ok(())
}
