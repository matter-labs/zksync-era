use anyhow::{anyhow, Context};
use common::{
    check_prerequisites, cmd::Cmd, config::global_config, logger, spinner::Spinner,
    GPU_PREREQUISITES,
};
use config::{traits::SaveConfigWithBasePath, ChainConfig, EcosystemConfig};
use xshell::{cmd, Shell};

use super::{
    args::run::{
        ProverComponent, ProverRunArgs, WitnessGeneratorArgs, WitnessGeneratorRound,
        WitnessVectorGeneratorArgs,
    },
    utils::get_link_to_prover,
};
use crate::{
    commands::prover::args::run::FriProverRunArgs,
    messages::{
        MSG_BELLMAN_CUDA_DIR_ERR, MSG_CHAIN_NOT_FOUND_ERR, MSG_MISSING_COMPONENT_ERR,
        MSG_RUNNING_COMPRESSOR, MSG_RUNNING_COMPRESSOR_ERR, MSG_RUNNING_PROVER,
        MSG_RUNNING_PROVER_ERR, MSG_RUNNING_PROVER_GATEWAY, MSG_RUNNING_PROVER_GATEWAY_ERR,
        MSG_RUNNING_PROVER_JOB_MONITOR, MSG_RUNNING_WITNESS_GENERATOR,
        MSG_RUNNING_WITNESS_GENERATOR_ERR, MSG_RUNNING_WITNESS_VECTOR_GENERATOR,
        MSG_RUNNING_WITNESS_VECTOR_GENERATOR_ERR, MSG_WITNESS_GENERATOR_ROUND_ERR,
    },
};

pub(crate) async fn run(args: ProverRunArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt()?;
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .expect(MSG_CHAIN_NOT_FOUND_ERR);

    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(link_to_prover.clone());

    let docker_image = args.component.map(|component| component.image_name());

    match args.component {
        Some(ProverComponent::Gateway) => run_gateway(shell, &chain, docker_image)?,
        Some(ProverComponent::WitnessGenerator) => {
            run_witness_generator(shell, &chain, args.witness_generator_args, docker_image)?
        }
        Some(ProverComponent::WitnessVectorGenerator) => run_witness_vector_generator(
            shell,
            &chain,
            args.witness_vector_generator_args,
            docker_image,
        )?,
        Some(ProverComponent::Prover) => {
            run_prover(shell, &chain, args.fri_prover_args, docker_image)?
        }
        Some(ProverComponent::Compressor) => {
            run_compressor(shell, &chain, &ecosystem_config, docker_image)?
        }
        Some(ProverComponent::ProverJobMonitor) => {
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
        run_dockerized_component(shell, docker_image, chain, "")
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
        run_dockerized_component(shell, docker_image, chain, round_str)
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
        run_dockerized_component(shell, docker_image, chain, &format!("--threads={threads}"))
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
    fri_prover_run_args: FriProverRunArgs,
    docker_image: Option<&str>,
) -> anyhow::Result<()> {
    logger::info(MSG_RUNNING_PROVER);
    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    let additional_args = if let Some(max_allocation) = fri_prover_run_args.max_allocation {
        format!("--max-allocation={max_allocation}")
    } else {
        "".to_string()
    };

    if let Some(docker_image) = docker_image {
        change_setup_data_path(shell, chain, "prover/data/keys")?;
        run_dockerized_component(shell, docker_image, chain, &additional_args)
    } else {
        check_prerequisites(shell, &GPU_PREREQUISITES, false);
        change_setup_data_path(shell, chain, "data/keys")?;
        check_prerequisites(shell, &GPU_PREREQUISITES, false);
        let mut cmd = if additional_args.is_empty() {
            Cmd::new(
                cmd!(shell, "cargo run --features gpu --release --bin zksync_prover_fri -- --config-path={config_path} --secrets-path={secrets_path}"),
            )
        } else {
            Cmd::new(
                cmd!(shell, "cargo run --features gpu --release --bin zksync_prover_fri -- --config-path={config_path} --secrets-path={secrets_path} {additional_args}"),
            )
        };
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
        run_dockerized_component(shell, docker_image, chain, "")
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
        run_dockerized_component(shell, docker_image, chain, "")
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
    chain_config: &ChainConfig,
    additional_args: &str,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(&format!("Pulling image {}...", image_name));
    let pull_cmd = Cmd::new(cmd!(shell, "docker pull {image_name}"));
    pull_cmd.run()?;
    spinner.finish();

    let path_to_configs = chain_config.configs.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let path_to_prover = get_link_to_prover(&ecosystem_config);

    let mut cmd = if additional_args.is_empty() {
        Cmd::new(cmd!(
            shell,
            "docker run --net=host --gpus=all -v {path_to_prover}/data/keys:/prover/data/keys -v {path_to_prover}/artifacts:/artifacts -v {path_to_configs}:/configs {image_name} --config-path=/configs/general.yaml --secrets-path=/configs/secrets.yaml"
        ))
    } else {
        Cmd::new(cmd!(
            shell,
            "docker run --net=host --gpus=all -v {path_to_prover}/data/keys:/prover/data/keys -v {path_to_prover}/artifacts:/artifacts -v {path_to_configs}:/configs {image_name} --config-path=/configs/general.yaml --secrets-path=/configs/secrets.yaml {additional_args}"
        ))
    };
    cmd = cmd.with_force_run();
    cmd.run().map_err(|err| anyhow!(err))
}

fn change_setup_data_path(
    shell: &Shell,
    chain_config: &ChainConfig,
    path: &str,
) -> anyhow::Result<()> {
    let mut general_config = chain_config.get_general_config()?;

    if let Some(prover_config) = general_config.prover_config.as_mut() {
        prover_config.setup_data_path = path.to_string();
    }

    general_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

postgres://postgres:notsecurepassword@localhost:5432/zksync_prover_localhost_test