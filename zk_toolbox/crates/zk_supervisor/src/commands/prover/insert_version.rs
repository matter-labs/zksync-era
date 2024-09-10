use common::{check_prerequisites, cmd::Cmd, logger, PROVER_CLI_PREREQUISITE};
use config::{get_link_to_prover, EcosystemConfig};
use xshell::{cmd, Shell};

use crate::commands::prover::{
    args::insert_version::{InsertVersionArgs, InsertVersionArgsFinal},
    info,
};

pub async fn run(shell: &Shell, args: InsertVersionArgs) -> anyhow::Result<()> {
    check_prerequisites(shell, &PROVER_CLI_PREREQUISITE, false);

    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let version = info::get_protocol_version(shell, &get_link_to_prover(&ecosystem_config)).await?;
    let snark_wrapper = info::get_snark_wrapper(&get_link_to_prover(&ecosystem_config)).await?;

    let prover_url = info::get_database_url(shell).await?;

    let InsertVersionArgsFinal {
        version,
        snark_wrapper,
    } = args.fill_values_with_prompts(version, snark_wrapper);

    let (minor, patch) = info::parse_version(&version)?;

    logger::info(format!(
        "Inserting protocol version {}, snark wrapper {} into the database",
        version, snark_wrapper
    ));

    let cmd = Cmd::new(cmd!(shell, "prover_cli {prover_url} insert-version --version={minor} --patch={patch} --snark-wrapper={snark_wrapper}"));
    cmd.run()?;

    logger::info("Done.");

    Ok(())
}
