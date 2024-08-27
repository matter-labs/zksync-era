use anyhow::Context;
use common::{check_prover_prequisites, cmd::Cmd, logger};
use config::{ChainConfig, EcosystemConfig};
use xshell::{cmd, Shell};

use super::{
    args::run::{
        ProverComponent, ProverRunArgs, WitnessGeneratorArgs, WitnessGeneratorRound,
        WitnessVectorGeneratorArgs,
    },
    utils::get_link_to_prover,
};
use crate::messages::{
    MSG_BELLMAN_CUDA_DIR_ERR, MSG_CHAIN_NOT_FOUND_ERR, MSG_MISSING_COMPONENT_ERR,
    MSG_RUNNING_COMPRESSOR, MSG_RUNNING_COMPRESSOR_ERR, MSG_RUNNING_PROVER, MSG_RUNNING_PROVER_ERR,
    MSG_RUNNING_PROVER_GATEWAY, MSG_RUNNING_PROVER_GATEWAY_ERR, MSG_RUNNING_PROVER_JOB_MONITOR,
    MSG_RUNNING_WITNESS_GENERATOR, MSG_RUNNING_WITNESS_GENERATOR_ERR,
    MSG_RUNNING_WITNESS_VECTOR_GENERATOR, MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR,
    MSG_WITNESS_GENERATOR_ROUND_ERR,
};

pub(crate) async fn run(args: ProverRunArgs, shell: &Shell) -> anyhow::Result<()> {
    check_prover_prequisites(shell);
    let args = args.fill_values_with_prompt()?;
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem_config
        .load_chain(Some(ecosystem_config.default_chain.clone()))
        .expect(MSG_CHAIN_NOT_FOUND_ERR);

    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(link_to_prover.clone());

    match args.component {
        Some(ProverComponent::Gateway) => run_gateway(shell, &chain)?,
        Some(ProverComponent::WitnessGenerator) => {
            run_witness_generator(shell, &chain, args.witness_generator_args)?
        }
        Some(ProverComponent::WitnessVectorGenerator) => {
            run_witness_vector_generator(shell, &chain, args.witness_vector_generator_args)?
        }
        Some(ProverComponent::Prover) => run_prover(shell, &chain)?,
        Some(ProverComponent::Compressor) => run_compressor(shell, &chain, &ecosystem_config)?,
        Some(ProverComponent::ProverJobMonitor) => run_prover_job_monitor(shell, &chain)?,
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
    cmd.run().context(MSG_RUNNING_PROVER_GATEWAY_ERR)
}

fn run_witness_generator(
    shell: &Shell,
    chain: &ChainConfig,
    args: WitnessGeneratorArgs,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_GENERATOR);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();
    let round = args.round.expect(MSG_WITNESS_GENERATOR_ROUND_ERR);

    let round_str = match round {
        WitnessGeneratorRound::AllRounds => "--all_rounds",
        WitnessGeneratorRound::BasicCircuits => "--round=basic_circuits",
        WitnessGeneratorRound::LeafAggregation => "--round=leaf_aggregation",
        WitnessGeneratorRound::NodeAggregation => "--round=node_aggregation",
        WitnessGeneratorRound::RecursionTip => "--round=recursion_tip",
        WitnessGeneratorRound::Scheduler => "--round=scheduler",
    };

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_witness_generator -- {round_str} --config-path={config_path} --secrets-path={secrets_path}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_WITNESS_GENERATOR_ERR)
}

fn run_witness_vector_generator(
    shell: &Shell,
    chain: &ChainConfig,
    args: WitnessVectorGeneratorArgs,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_VECTOR_GENERATOR);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let threads = args.threads.unwrap_or(1).to_string();
    let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_witness_vector_generator -- --config-path={config_path} --secrets-path={secrets_path} --threads={threads}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR)
}

fn run_prover(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let mut cmd = Cmd::new(
        cmd!(shell, "cargo run --features gpu --release --bin zksync_prover_fri -- --config-path={config_path} --secrets-path={secrets_path}"),
    );
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_PROVER_ERR)
}

fn run_compressor(
    shell: &Shell,
    chain: &ChainConfig,
    ecosystem: &EcosystemConfig,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_COMPRESSOR);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    shell.set_var(
        "BELLMAN_CUDA_DIR",
        ecosystem
            .bellman_cuda_dir
            .clone()
            .expect(MSG_BELLMAN_CUDA_DIR_ERR),
    );

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --features gpu --release --bin zksync_proof_fri_compressor -- --config-path={config_path} --secrets-path={secrets_path}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_COMPRESSOR_ERR)
}

fn run_prover_job_monitor(shell: &Shell, chain: &ChainConfig) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER_JOB_MONITOR);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_prover_job_monitor -- --config-path={config_path} --secrets-path={secrets_path}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_PROVER_JOB_MONITOR)
}
