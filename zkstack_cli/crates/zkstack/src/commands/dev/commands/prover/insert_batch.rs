use xshell::{cmd, Shell};
use zkstack_cli_common::{check_prerequisites, cmd::Cmd, logger, PROVER_CLI_PREREQUISITE};
use zkstack_cli_config::{get_link_to_prover, ZkStackConfig};

use crate::commands::dev::commands::prover::{
    args::insert_batch::{InsertBatchArgs, InsertBatchArgsFinal},
    info,
};

pub async fn run(shell: &Shell, args: InsertBatchArgs) -> anyhow::Result<()> {
    check_prerequisites(shell, &PROVER_CLI_PREREQUISITE, false);

    let chain_config = ZkStackConfig::current_chain(shell)?;

    let version =
        info::get_protocol_version(shell, &get_link_to_prover(&chain_config.link_to_code)).await?;
    let prover_url = info::get_database_url(&chain_config).await?.to_string();

    let InsertBatchArgsFinal { number, version } = args.fill_values_with_prompts(version);

    let (minor, patch) = info::parse_version(&version)?;

    logger::info(format!(
        "Inserting protocol version {}, batch number {} into the database",
        version, number
    ));

    let number = number.to_string();

    let cmd = Cmd::new(cmd!(
        shell,
        "prover_cli {prover_url} insert-batch --version={minor} --patch={patch} --number={number}"
    ));
    cmd.run()?;

    logger::info("Done.");

    Ok(())
}
