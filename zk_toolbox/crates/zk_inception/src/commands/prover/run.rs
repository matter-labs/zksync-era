use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use common::{cmd::Cmd, logger};
use config::{ChainConfig, EcosystemConfig};
use xshell::{cmd, Shell};

use super::{
    args::run::{ProverComponent, ProverRunArgs},
    utils::get_link_to_prover,
};
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_MISSING_COMPONENTS_ERR, MSG_RUNNING_COMPRESSOR,
    MSG_RUNNING_PROVER, MSG_RUNNING_PROVER_GATEWAY, MSG_RUNNING_WITNESS_GENERATOR,
    MSG_RUNNING_WITNESS_VECTOR_GENERATOR,
};

pub(crate) async fn run(args: ProverRunArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt()?;
    logger::debug(format!("Prover args: {:?}", args));
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem_config
        .load_chain(Some(ecosystem_config.default_chain.clone()))
        .expect(MSG_CHAIN_NOT_FOUND_ERR);

    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(&link_to_prover);

    for component in args.components.expect(MSG_MISSING_COMPONENTS_ERR) {
        match component {
            ProverComponent::Gateway => run_gateway(shell, &chain)?,
            ProverComponent::WitnessGenerator => run_witness_generator(shell)?,
            ProverComponent::WitnessVectorGenerator => run_witness_vector_generator(shell)?,
            ProverComponent::Prover => run_prover(shell)?,
            ProverComponent::Compressor => run_compressor(shell)?,
        }
    }

    logger::outro("Prover components started successfully".to_string());

    Ok(())
}

fn run_gateway(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER_GATEWAY);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let command_str = format!(
        "cargo run --release --bin zksync_prover_fri_gateway -- --config-path={} --secrets-path={}",
        config_path.to_str().unwrap(),
        secrets_path.to_str().unwrap()
    );

    let cmd = Command::new("sh")
        .arg("-c")
        .arg(&command_str)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Get the PID of the spawned process
    let pid = cmd.id();

    logger::info(format!("Prover gateway started with pid: {}", pid));
    Ok(())
}

fn run_witness_generator(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_GENERATOR);
    todo!()
}

fn run_witness_vector_generator(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_VECTOR_GENERATOR);
    todo!()
}

fn run_prover(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER);
    todo!()
}

fn run_compressor(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_COMPRESSOR);
    todo!()
}
