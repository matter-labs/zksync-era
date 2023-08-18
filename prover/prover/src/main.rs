use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

use api::gpu_prover;
use local_ip_address::local_ip;
use prover_service::run_prover::run_prover_with_remote_synthesizer;
use queues::Buffer;
use tokio::{sync::oneshot, task::JoinHandle};

use zksync_config::{
    configs::{api::PrometheusConfig, prover_group::ProverGroupConfig, AlertsConfig},
    ApiConfig, ProverConfig, ProverConfigs,
};
use zksync_dal::{connection::DbVariant, ConnectionPool};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_utils::region_fetcher::{get_region, get_zone};
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};
use zksync_utils::wait_for_tasks::wait_for_tasks;

use crate::artifact_provider::ProverArtifactProvider;
use crate::prover::ProverReporter;
use crate::prover_params::ProverParams;
use crate::socket_listener::incoming_socket_listener;
use crate::synthesized_circuit_provider::SynthesizedCircuitProvider;

mod artifact_provider;
mod prover;
mod prover_params;
mod socket_listener;
mod synthesized_circuit_provider;

async fn graceful_shutdown() {
    let pool = ConnectionPool::singleton(DbVariant::Prover).build().await;
    let host = local_ip().expect("Failed obtaining local IP address");
    let port = ProverConfigs::from_env().non_gpu.assembly_receiver_port;
    let region = get_region().await;
    let zone = get_zone().await;
    let address = SocketAddress { host, port };
    pool.clone()
        .access_storage()
        .await
        .gpu_prover_queue_dal()
        .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, 0, region, zone)
        .await;
}

fn get_ram_per_gpu() -> u64 {
    let device_info = gpu_prover::cuda_bindings::device_info(0).unwrap();
    let ram_in_gb: u64 = device_info.total / (1024 * 1024 * 1024);
    vlog::info!("Detected RAM per GPU: {:?} GB", ram_in_gb);
    ram_in_gb
}

fn get_prover_config_for_machine_type() -> (ProverConfig, u8) {
    let prover_configs = ProverConfigs::from_env();
    let actual_num_gpus = match gpu_prover::cuda_bindings::devices() {
        Ok(gpus) => gpus as u8,
        Err(err) => {
            vlog::error!("unable to get number of GPUs: {err:?}");
            panic!("unable to get number of GPUs: {:?}", err);
        }
    };
    vlog::info!("detected number of gpus: {}", actual_num_gpus);
    let ram_in_gb = get_ram_per_gpu();

    match actual_num_gpus {
        1 => {
            vlog::info!("Detected machine type with 1 GPU and 80GB RAM");
            (prover_configs.one_gpu_eighty_gb_mem, actual_num_gpus)
        }
        2 => {
            if ram_in_gb > 39 {
                vlog::info!("Detected machine type with 2 GPU and 80GB RAM");
                (prover_configs.two_gpu_eighty_gb_mem, actual_num_gpus)
            } else {
                vlog::info!("Detected machine type with 2 GPU and 40GB RAM");
                (prover_configs.two_gpu_forty_gb_mem, actual_num_gpus)
            }
        }
        4 => {
            vlog::info!("Detected machine type with 4 GPU and 80GB RAM");
            (prover_configs.four_gpu_eighty_gb_mem, actual_num_gpus)
        }
        _ => panic!("actual_num_gpus: {} not supported yet", actual_num_gpus),
    }
}

#[tokio::main]
async fn main() {
    vlog::init();
    vlog::trace!("starting prover");
    let (prover_config, num_gpu) = get_prover_config_for_machine_type();

    let prometheus_config = PrometheusConfig {
        listener_port: prover_config.prometheus_port,
        ..ApiConfig::from_env().prometheus
    };

    let region = get_region().await;
    let zone = get_zone().await;

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    zksync_prover_utils::ensure_initial_setup_keys_present(
        &prover_config.initial_setup_key_path,
        &prover_config.key_download_url,
    );
    env::set_var("CRS_FILE", prover_config.initial_setup_key_path.clone());
    // We don't have a graceful shutdown process for the prover, so `_stop_sender` is unused.
    // Though we still need to create a channel because circuit breaker expects `stop_receiver`.
    let (_stop_sender, stop_receiver) = tokio::sync::watch::channel(false);

    let circuit_ids = ProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(prover_config.specialized_prover_group_id);

    vlog::info!(
        "Starting proof generation for circuits: {circuit_ids:?} \
         in region: {region} and zone: {zone} with group-id: {}",
        prover_config.specialized_prover_group_id
    );
    let mut tasks: Vec<JoinHandle<()>> = vec![];

    tasks.push(prometheus_exporter::run_prometheus_exporter(
        prometheus_config.listener_port,
        None,
    ));

    let assembly_queue = Buffer::new(prover_config.assembly_queue_capacity);
    let shared_assembly_queue = Arc::new(Mutex::new(assembly_queue));
    let producer = shared_assembly_queue.clone();
    let consumer = shared_assembly_queue.clone();
    let local_ip = local_ip().expect("Failed obtaining local IP address");
    let address = SocketAddress {
        host: local_ip,
        port: prover_config.assembly_receiver_port,
    };
    vlog::info!("local IP address is: {:?}", local_ip);

    tasks.push(tokio::task::spawn(incoming_socket_listener(
        local_ip,
        prover_config.assembly_receiver_port,
        producer,
        ConnectionPool::singleton(DbVariant::Prover).build().await,
        prover_config.specialized_prover_group_id,
        region.clone(),
        zone.clone(),
        num_gpu,
    )));

    let params = ProverParams::new(&prover_config);
    let store_factory = ObjectStoreFactory::from_env();

    let circuit_provider_pool = ConnectionPool::singleton(DbVariant::Prover).build().await;
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
        let prover_job_reporter = ProverReporter::new(prover_config, &store_factory, rt_handle);
        run_prover_with_remote_synthesizer(
            synthesized_circuit_provider,
            ProverArtifactProvider,
            prover_job_reporter,
            circuit_ids,
            params,
        )
    }));

    let particular_crypto_alerts = Some(AlertsConfig::from_env().sporadic_crypto_errors_substrs);
    let graceful_shutdown = Some(graceful_shutdown());
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            vlog::info!("Stop signal received, shutting down");

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
