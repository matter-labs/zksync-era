use std::path::PathBuf;

use anyhow::Context;
use common::{check_prover_prequisites, cmd::Cmd, logger};
use config::{is_prover_only_system, EcosystemConfig, GeneralProverConfig};
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
    MSG_RUNNING_PROVER_GATEWAY, MSG_RUNNING_PROVER_GATEWAY_ERR, MSG_RUNNING_WITNESS_GENERATOR,
    MSG_RUNNING_WITNESS_GENERATOR_ERR, MSG_RUNNING_WITNESS_VECTOR_GENERATOR,
    MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR, MSG_WITNESS_GENERATOR_ROUND_ERR,
};

pub(crate) async fn run(args: ProverRunArgs, shell: &Shell) -> anyhow::Result<()> {
    check_prover_prequisites(shell);
    let args = args.fill_values_with_prompt()?;

    let (link_to_code, config_path, secrets_path, bellman_cuda_dir_path) =
        if is_prover_only_system(shell)? {
            let general_prover_config = GeneralProverConfig::from_file(shell)?;
            (
                general_prover_config.link_to_code.clone(),
                general_prover_config.path_to_prover_config().clone(),
                general_prover_config.path_to_secrets().clone(),
                general_prover_config
                    .bellman_cuda_dir
                    .expect(MSG_BELLMAN_CUDA_DIR_ERR),
            )
        } else {
            let ecosystem_config = EcosystemConfig::from_file(shell)?;
            let chain = ecosystem_config
                .load_chain(Some(ecosystem_config.default_chain.clone()))
                .context(MSG_CHAIN_NOT_FOUND_ERR)?;
            (
                ecosystem_config.link_to_code,
                chain.path_to_general_config(),
                chain.path_to_secrets_config(),
                ecosystem_config
                    .bellman_cuda_dir
                    .expect(MSG_BELLMAN_CUDA_DIR_ERR),
            )
        };

    let link_to_prover = get_link_to_prover(link_to_code.clone());
    shell.change_dir(link_to_prover.clone());

    match args.component {
        Some(ProverComponent::Gateway) => run_gateway(shell, config_path, secrets_path)?,
        Some(ProverComponent::WitnessGenerator) => run_witness_generator(
            shell,
            config_path,
            secrets_path,
            args.witness_generator_args,
        )?,
        Some(ProverComponent::WitnessVectorGenerator) => run_witness_vector_generator(
            shell,
            config_path,
            secrets_path,
            args.witness_vector_generator_args,
        )?,
        Some(ProverComponent::Prover) => run_prover(shell, config_path, secrets_path)?,
        Some(ProverComponent::Compressor) => {
            run_compressor(shell, config_path, secrets_path, bellman_cuda_dir_path)?
        }
        None => anyhow::bail!(MSG_MISSING_COMPONENT_ERR),
    }

    Ok(())
}

fn run_gateway(shell: &Shell, config_path: PathBuf, secrets_path: PathBuf) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER_GATEWAY);
    let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_prover_fri_gateway -- --config-path={config_path} --secrets-path={secrets_path}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_PROVER_GATEWAY_ERR)
}

fn run_witness_generator(
    shell: &Shell,
    config_path: PathBuf,
    secrets_path: PathBuf,
    args: WitnessGeneratorArgs,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_GENERATOR);
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
    config_path: PathBuf,
    secrets_path: PathBuf,
    args: WitnessVectorGeneratorArgs,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_VECTOR_GENERATOR);
    let threads = args.threads.unwrap_or(1).to_string();
    let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_witness_vector_generator -- --config-path={config_path} --secrets-path={secrets_path} --threads={threads}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR)
}

fn run_prover(shell: &Shell, config_path: PathBuf, secrets_path: PathBuf) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER);

    let mut cmd = Cmd::new(
        cmd!(shell, "cargo run --features gpu --release --bin zksync_prover_fri -- --config-path={config_path} --secrets-path={secrets_path}"),
    );
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_PROVER_ERR)
}

fn run_compressor(
    shell: &Shell,
    config_path: PathBuf,
    secrets_path: PathBuf,
    bellman_cuda_dir_path: PathBuf,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_COMPRESSOR);

    shell.set_var("BELLMAN_CUDA_DIR", bellman_cuda_dir_path);

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --features gpu --release --bin zksync_proof_fri_compressor -- --config-path={config_path} --secrets-path={secrets_path}"));
    cmd = cmd.with_force_run();
    cmd.run().context(MSG_RUNNING_COMPRESSOR_ERR)
}
