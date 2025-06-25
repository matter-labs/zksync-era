use xshell::{cmd, Shell};
use zkstack_cli_common::{check_prerequisites, cmd::Cmd, logger, PROVER_CLI_PREREQUISITE};
use zkstack_cli_config::{get_link_to_prover, ZkStackConfig};

use crate::commands::dev::commands::prover::{
    args::insert_version::{InsertVersionArgs, InsertVersionArgsFinal},
    info,
};

pub async fn run(shell: &Shell, args: InsertVersionArgs) -> anyhow::Result<()> {
    check_prerequisites(shell, &PROVER_CLI_PREREQUISITE, false);

    let chain_config = ZkStackConfig::current_chain(shell)?;

    let version = info::get_protocol_version(
        shell,
        &get_link_to_prover(&chain_config.link_to_code.clone()),
    )
    .await?;
    let snark_wrapper =
        info::get_snark_wrapper(&get_link_to_prover(&chain_config.link_to_code.clone())).await?;
    let fflonk_snark_wrapper =
        info::get_fflonk_snark_wrapper(&get_link_to_prover(&chain_config.link_to_code.clone()))
            .await?;

    let prover_url = info::get_database_url(&chain_config).await?.to_string();

    let InsertVersionArgsFinal {
        version,
        snark_wrapper,
        fflonk_snark_wrapper,
    } = args.fill_values_with_prompts(version, snark_wrapper, fflonk_snark_wrapper);

    let (minor, patch) = info::parse_version(&version)?;

    logger::info(format!(
        "Inserting protocol version {}, snark wrapper {} into the database",
        version, snark_wrapper
    ));

    let cmd = Cmd::new(cmd!(shell, "prover_cli {prover_url} insert-version --version={minor} --patch={patch} --snark-wrapper={snark_wrapper} --fflonk-snark-wrapper={fflonk_snark_wrapper}"));
    cmd.run()?;

    logger::info("Done.");

    Ok(())
}
