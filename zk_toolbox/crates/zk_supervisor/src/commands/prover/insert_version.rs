use crate::commands::prover::info;
use common::cmd::Cmd;
use common::{check_prerequisites, logger, PROVER_CLI_PREREQUISITE};
use config::{get_link_to_prover, EcosystemConfig};
use xshell::{cmd, Shell};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    check_prerequisites(shell, &PROVER_CLI_PREREQUISITE, false);

    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let version = info::get_protocol_version(shell, &get_link_to_prover(&ecosystem_config)).await?;
    let snark_wrapper = info::get_snark_wrapper(&get_link_to_prover(&ecosystem_config)).await?;

    let prover_url = info::get_database_url(shell).await?;

    let (minor, patch) = parse_version(&version);

    logger::info(format!(
        "Inserting protocol version {}, snark wrapper {} into the database",
        version, snark_wrapper
    ));

    let cmd = Cmd::new(cmd!(shell, "prover_cli {prover_url} insert-version --version={minor} --patch={patch} --snark-wrapper={snark_wrapper}"));
    cmd.run()?;

    logger::info("Done.");

    Ok(())
}

fn parse_version(version: &str) -> (&str, &str) {
    let splitted: Vec<&str> = version.split(".").collect();
    let minor = splitted[1];
    let patch = splitted[2];

    (minor, patch)
}
