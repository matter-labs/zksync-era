use anyhow::Context;
use common::{cmd::Cmd, logger};
use config::{ChainConfig, EcosystemConfig};
use xshell::{cmd, Shell};

use super::{
    args::run::{ProverComponent, ProverRunArgs},
    utils::get_link_to_prover,
};
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_MISSING_COMPONENT_ERR, MSG_RUNNING_COMPRESSOR, MSG_RUNNING_PROVER,
    MSG_RUNNING_PROVER_GATEWAY, MSG_RUNNING_PROVER_GATEWAY_ERR, MSG_RUNNING_WITNESS_GENERATOR,
    MSG_RUNNING_WITNESS_VECTOR_GENERATOR,
};

pub(crate) async fn run(args: ProverRunArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt()?;
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem_config
        .load_chain(Some(ecosystem_config.default_chain.clone()))
        .expect(MSG_CHAIN_NOT_FOUND_ERR);

    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(link_to_prover.clone());

    match args.component {
        Some(ProverComponent::Gateway) => run_gateway(shell, &chain)?,
        Some(ProverComponent::WitnessGenerator) => run_witness_generator(shell, &chain)?,
        Some(ProverComponent::WitnessVectorGenerator) => {
            run_witness_vector_generator(shell, &chain)?
        }
        Some(ProverComponent::Prover) => run_prover(shell, &chain)?,
        Some(ProverComponent::Compressor) => run_compressor(shell, &chain)?,
        None => anyhow::bail!(MSG_MISSING_COMPONENT_ERR),
    }

    Ok(())
}

fn run_gateway(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER_GATEWAY);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_prover_fri_gateway -- --config-path={config_path} --secrets-path={secrets_path}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_PROVER_GATEWAY_ERR)?;

    Ok(())
}

fn run_witness_generator(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_GENERATOR);
    todo!()
}

fn run_witness_vector_generator(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_VECTOR_GENERATOR);
    todo!()
}

fn run_prover(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER);
    todo!()
}

fn run_compressor(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_COMPRESSOR);
    todo!()
}
