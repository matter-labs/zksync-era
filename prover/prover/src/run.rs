use anyhow::Context as _;
use std::{env, future::Future, sync::Arc, time::Instant};
use tokio::sync::{oneshot, Mutex};

use local_ip_address::local_ip;
use queues::Buffer;

use prometheus_exporter::PrometheusExporterConfig;
use zksync_config::{
    configs::{api::PrometheusConfig, prover_group::ProverGroupConfig, AlertsConfig},
    ApiConfig, ProverConfig, ProverConfigs,
};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{connection::DbVariant, ProverConnectionPool};
use zksync_prover_utils::region_fetcher::{get_region, get_zone};
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};
use zksync_utils::wait_for_tasks::wait_for_tasks;

use crate::artifact_provider::ProverArtifactProvider;
use crate::prover::ProverReporter;
use crate::prover_params::ProverParams;
use crate::socket_listener::incoming_socket_listener;
use crate::synthesized_circuit_provider::SynthesizedCircuitProvider;

async fn graceful_shutdown() -> anyhow::Result<impl Future<Output = ()>> {
    let pool = ProverConnectionPool::singleton(DbVariant::Real)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let host = local_ip().context("Failed obtaining local IP address")?;
    let port = ProverConfigs::from_env()
        .context("ProverConfigs")?
        .non_gpu
        .assembly_receiver_port;
    let region = get_region().await.context("get_region()")?;
    let zone = get_zone().await.context("get_zone()")?;
    let address = SocketAddress { host, port };
    Ok(async move {
        pool.access_storage()
            .await
            .unwrap()
            .gpu_prover_queue_dal()
            .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, 0, region, zone)
            .await
    })
}

fn get_ram_per_gpu() -> anyhow::Result<u64> {
    use api::gpu_prover::cuda_bindings;

    let device_info =
        cuda_bindings::device_info(0).map_err(|err| anyhow::anyhow!("device_info(): {err:?}"))?;
    let ram_in_gb: u64 = device_info.total / (1024 * 1024 * 1024);
    tracing::info!("Detected RAM per GPU: {:?} GB", ram_in_gb);
    Ok(ram_in_gb)
}

fn get_prover_config_for_machine_type() -> anyhow::Result<(ProverConfig, u8)> {
    use api::gpu_prover::cuda_bindings;

    let prover_configs = ProverConfigs::from_env().context("ProverConfigs::from_env()")?;
    let actual_num_gpus = match cuda_bindings::devices() {
        Ok(gpus) => gpus as u8,
        Err(err) => {
            tracing::error!("unable to get number of GPUs: {err:?}");
            anyhow::bail!("unable to get number of GPUs: {:?}", err);
        }
    };
    tracing::info!("detected number of gpus: {}", actual_num_gpus);
    let ram_in_gb = get_ram_per_gpu().context("get_ram_per_gpu()")?;

    Ok(match actual_num_gpus {
        1 => {
            tracing::info!("Detected machine type with 1 GPU and 80GB RAM");
            (prover_configs.one_gpu_eighty_gb_mem, actual_num_gpus)
        }
        2 => {
            if ram_in_gb > 39 {
                tracing::info!("Detected machine type with 2 GPU and 80GB RAM");
                (prover_configs.two_gpu_eighty_gb_mem, actual_num_gpus)
            } else {
                tracing::info!("Detected machine type with 2 GPU and 40GB RAM");
                (prover_configs.two_gpu_forty_gb_mem, actual_num_gpus)
            }
        }
        4 => {
            tracing::info!("Detected machine type with 4 GPU and 80GB RAM");
            (prover_configs.four_gpu_eighty_gb_mem, actual_num_gpus)
        }
        _ => anyhow::bail!("actual_num_gpus: {} not supported yet", actual_num_gpus),
    })
}

pub async fn run() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    tracing::trace!("starting prover");
    let (prover_config, num_gpu) =
        get_prover_config_for_machine_type().context("get_prover_config_for_machine_type()")?;

    let prometheus_config = PrometheusConfig {
        listener_port: prover_config.prometheus_port,
        ..ApiConfig::from_env()
            .context("ApiConfig::from_env()")?
            .prometheus
    };

    let region = get_region().await.context("get_regtion()")?;
    let zone = get_zone().await.context("get_zone()")?;

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    let started_at = Instant::now();
    zksync_prover_utils::ensure_initial_setup_keys_present(
        &prover_config.initial_setup_key_path,
        &prover_config.key_download_url,
    );
    metrics::histogram!("server.prover.download_time", started_at.elapsed());

    env::set_var("CRS_FILE", prover_config.initial_setup_key_path.clone());
    // We don't have a graceful shutdown process for the prover, so `_stop_sender` is unused.
    // Though we still need to create a channel because circuit breaker expects `stop_receiver`.
    let (_stop_sender, stop_receiver) = tokio::sync::watch::channel(false);

    let circuit_ids = ProverGroupConfig::from_env()
        .context("ProverGroupConfig::from_env()")?
        .get_circuit_ids_for_group_id(prover_config.specialized_prover_group_id);

    tracing::info!(
        "Starting proof generation for circuits: {circuit_ids:?} \
         in region: {region} and zone: {zone} with group-id: {}",
        prover_config.specialized_prover_group_id
    );
    let mut tasks = vec![];

    let exporter_config = PrometheusExporterConfig::pull(prometheus_config.listener_port);
    tasks.push(tokio::spawn(exporter_config.run(stop_receiver.clone())));

    let assembly_queue = Buffer::new(prover_config.assembly_queue_capacity);
    let shared_assembly_queue = Arc::new(Mutex::new(assembly_queue));
    let producer = shared_assembly_queue.clone();
    let consumer = shared_assembly_queue.clone();
    let local_ip = local_ip().context("Failed obtaining local IP address")?;
    let address = SocketAddress {
        host: local_ip,
        port: prover_config.assembly_receiver_port,
    };
    tracing::info!("local IP address is: {:?}", local_ip);

    tasks.push(tokio::task::spawn(incoming_socket_listener(
        local_ip,
        prover_config.assembly_receiver_port,
        producer,
        ProverConnectionPool::singleton(DbVariant::Real)
            .build()
            .await
            .context("failed to build a connection pool")?,
        prover_config.specialized_prover_group_id,
        region.clone(),
        zone.clone(),
        num_gpu,
    )));

    let params = ProverParams::new(&prover_config);
    let store_factory = ObjectStoreFactory::from_env().context("ObjectStoreFactory::from_env()")?;

    let circuit_provider_pool = ProverConnectionPool::singleton(DbVariant::Real)
        .build()
        .await
        .context("failed to build circuit_provider_pool")?;
    tasks.push(tokio::task::spawn_blocking(move || {
        let rt_handle = tokio::runtime::Handle::current();
        let synthesized_circuit_provider = SynthesizedCircuitProvider::new(
            consumer,
            circuit_provider_pool,
            address,
            region,
            zone,
            rt_handle.clone(),
        );
        let prover_job_reporter = ProverReporter::new(prover_config, &store_factory, rt_handle)
            .context("ProverReporter::new()")?;
        prover_service::run_prover::run_prover_with_remote_synthesizer(
            synthesized_circuit_provider,
            ProverArtifactProvider,
            prover_job_reporter,
            circuit_ids,
            params,
        );
        Ok(())
    }));

    let particular_crypto_alerts = Some(
        AlertsConfig::from_env()
            .context("AlertsConfig::from_env()")?
            .sporadic_crypto_errors_substrs,
    );
    let graceful_shutdown = Some(
        graceful_shutdown()
            .await
            .context("failed to prepare graceful shutdown future")?,
    );
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");

            // BEWARE, HERE BE DRAGONS.
            // This is necessary because of blocking prover. See end of functions for more details.
            std::process::exit(0);
        },
    };

    // BEWARE, HERE BE DRAGONS.
    // The process hangs here if we panic outside `run_prover_with_remote_synthesizer`.
    // Given the task is spawned as blocking, it's in a different thread that can't be cancelled on demand.
    // See: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html for more information
    // Follow [PR](https://github.com/matter-labs/zksync-2-dev/pull/2129) for logic behind it
    std::process::exit(-1);
}
