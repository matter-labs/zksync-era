use anyhow::{anyhow, Context};
use common::spinner::Spinner;
use common::{check_prerequisites, cmd::Cmd, config::global_config, logger, GPU_PREREQUISITES};
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
    let args = args.fill_values_with_prompt()?;
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .expect(MSG_CHAIN_NOT_FOUND_ERR);

    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(link_to_prover.clone());

    match args.component {
        Some(ProverComponent::Gateway) => {
            let docker_image = if args.docker.unwrap_or(false) {
                Some(ProverComponent::image_name(&ProverComponent::Gateway))
            } else {
                None
            };
            run_gateway(shell, &chain, docker_image)?
        }
        Some(ProverComponent::WitnessGenerator) => {
            let docker_image = if args.docker.unwrap_or(false) {
                Some(ProverComponent::image_name(
                    &ProverComponent::WitnessGenerator,
                ))
            } else {
                None
            };
            run_witness_generator(shell, &chain, args.witness_generator_args, docker_image)?
        }
        Some(ProverComponent::WitnessVectorGenerator) => {
            let docker_image = if args.docker.unwrap_or(false) {
                Some(ProverComponent::image_name(
                    &ProverComponent::WitnessVectorGenerator,
                ))
            } else {
                None
            };
            run_witness_vector_generator(
                shell,
                &chain,
                args.witness_vector_generator_args,
                docker_image,
            )?
        }
        Some(ProverComponent::Prover) => {
            let docker_image = if args.docker.unwrap_or(false) {
                Some(ProverComponent::image_name(&ProverComponent::Prover))
            } else {
                None
            };
            run_prover(shell, &chain, docker_image)?
        }
        Some(ProverComponent::Compressor) => {
            let docker_image = if args.docker.unwrap_or(false) {
                Some(ProverComponent::image_name(&ProverComponent::Compressor))
            } else {
                None
            };
            run_compressor(shell, &chain, &ecosystem_config, docker_image)?
        }
        Some(ProverComponent::ProverJobMonitor) => {
            let docker_image = if args.docker.unwrap_or(false) {
                Some(ProverComponent::image_name(
                    &ProverComponent::ProverJobMonitor,
                ))
            } else {
                None
            };
            run_prover_job_monitor(shell, &chain, docker_image)?
        }
        None => anyhow::bail!(MSG_MISSING_COMPONENT_ERR),
    }

    Ok(())
}

fn run_gateway(
    shell: &Shell,
    chain: &ChainConfig,
    docker_image: Option<&str>,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER_GATEWAY);

    if let Some(docker_image) = docker_image {
        run_dockerized_component(shell, docker_image, "", MSG_RUNNING_PROVER_GATEWAY_ERR)
    } else {
        let config_path = chain.path_to_general_config();
        let secrets_path = chain.path_to_secrets_config();

        let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_prover_fri_gateway -- --config-path={config_path} --secrets-path={secrets_path}"));
        cmd = cmd.with_force_run();
        cmd.run().context(MSG_RUNNING_PROVER_GATEWAY_ERR)
    }
}

fn run_witness_generator(
    shell: &Shell,
    chain: &ChainConfig,
    args: WitnessGeneratorArgs,
    docker_image: Option<&str>,
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

    if let Some(docker_image) = docker_image {
        run_dockerized_component(
            shell,
            docker_image,
            round_str,
            MSG_RUNNING_PROVER_GATEWAY_ERR,
        )
    } else {
        let config_path = chain.path_to_general_config();
        let secrets_path = chain.path_to_secrets_config();

        let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_witness_generator -- {round_str} --config-path={config_path} --secrets-path={secrets_path}"));
        cmd = cmd.with_force_run();
        cmd.run().context(MSG_RUNNING_WITNESS_GENERATOR_ERR)
    }
}

fn run_witness_vector_generator(
    shell: &Shell,
    chain: &ChainConfig,
    args: WitnessVectorGeneratorArgs,
    docker_image: Option<&str>,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_WITNESS_VECTOR_GENERATOR);
    let threads = args.threads.unwrap_or(1).to_string();

    if let Some(docker_image) = docker_image {
        run_dockerized_component(
            shell,
            docker_image,
            &format!("--threads={threads}"),
            MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR,
        )
    } else {
        let config_path = chain.path_to_general_config();
        let secrets_path = chain.path_to_secrets_config();

        let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_witness_vector_generator -- --config-path={config_path} --secrets-path={secrets_path} --threads={threads}"));
        cmd = cmd.with_force_run();
        cmd.run().context(MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR)
    }
}

fn run_prover(
    shell: &Shell,
    chain: &ChainConfig,
    docker_image: Option<&str>,
) -> anyhow::Result<()> {
    check_prerequisites(shell, &GPU_PREREQUISITES, false);
    logger::info(MSG_RUNNING_PROVER);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    if let Some(docker_image) = docker_image {
        run_dockerized_component(shell, docker_image, "", MSG_RUNNING_PROVER_ERR)
    } else {
        let mut cmd = Cmd::new(
            cmd!(shell, "cargo run --features gpu --release --bin zksync_prover_fri -- --config-path={config_path} --secrets-path={secrets_path}"),
        );
        cmd = cmd.with_force_run();
        cmd.run().context(MSG_RUNNING_PROVER_ERR)
    }
}

fn run_compressor(
    shell: &Shell,
    chain: &ChainConfig,
    ecosystem: &EcosystemConfig,
    docker_image: Option<&str>,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_COMPRESSOR);

    if let Some(docker_image) = docker_image {
        run_dockerized_component(shell, docker_image, "", MSG_RUNNING_COMPRESSOR_ERR)
    } else {
        check_prerequisites(shell, &GPU_PREREQUISITES, false);
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
}

fn run_prover_job_monitor(
    shell: &Shell,
    chain: &ChainConfig,
    docker_image: Option<&str>,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER_JOB_MONITOR);

    if let Some(docker_image) = docker_image {
        run_dockerized_component(shell, docker_image, "", MSG_RUNNING_PROVER_JOB_MONITOR)
    } else {
        let config_path = chain.path_to_general_config();
        let secrets_path = chain.path_to_secrets_config();

        let mut cmd = Cmd::new(cmd!(shell, "cargo run --release --bin zksync_prover_job_monitor -- --config-path={config_path} --secrets-path={secrets_path}"));
        cmd = cmd.with_force_run();
        cmd.run().context(MSG_RUNNING_PROVER_JOB_MONITOR)
    }
}

fn run_dockerized_component(
    shell: &Shell,
    image_name: &str,
    additional_args: &str,
    error: &str,
) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let spinner = Spinner::new(&format!("Pulling image {}...", image_name));
    let pull_cmd = Cmd::new(cmd!(shell, "docker pull {image_name}"));
    pull_cmd.run()?;
    spinner.finish();

    let path_to_configs = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .expect(MSG_CHAIN_NOT_FOUND_ERR)
        .configs
        .clone();
    let path_to_artifacts =
        get_link_to_prover(&EcosystemConfig::from_file(shell)?).join("/artifacts");

    let mut cmd = Cmd::new(cmd!(
            shell,
            "docker run {image_name} --net=host -v {path_to_artifacts}:/zksync-era/prover/artifacts -v {path_to_configs}:/configs --config-path=/configs/general.yaml --secrets-path=/configs/secrets.yaml {additional_args}"
        ));
    cmd = cmd.with_force_run();
    cmd.run().map_err(|err| anyhow!(err))
}
