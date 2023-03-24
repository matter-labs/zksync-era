use std::cell::RefCell;
use std::env;
use std::sync::{Arc, Mutex};

use api::gpu_prover;
use futures::{channel::mpsc, executor::block_on, future, SinkExt, StreamExt};
use local_ip_address::local_ip;
use prover_service::run_prover::run_prover_with_remote_synthesizer;
use queues::Buffer;
use tokio::task::JoinHandle;

use zksync_circuit_breaker::{vks::VksChecker, CircuitBreakerChecker};
use zksync_config::configs::prover_group::ProverGroupConfig;
use zksync_config::{
    configs::api::Prometheus as PrometheusConfig, ApiConfig, ProverConfig, ProverConfigs,
    ZkSyncConfig,
};
use zksync_dal::gpu_prover_queue_dal::{GpuProverInstanceStatus, SocketAddress};
use zksync_dal::ConnectionPool;

use crate::artifact_provider::ProverArtifactProvider;
use crate::prover::ProverReporter;
use crate::prover_params::ProverParams;
use zksync_prover_utils::region_fetcher::get_region;
use crate::socket_listener::incoming_socket_listener;
use crate::synthesized_circuit_provider::SynthesizedCircuitProvider;

mod artifact_provider;
mod prover;
mod prover_params;
mod socket_listener;
mod synthesized_circuit_provider;

pub async fn wait_for_tasks(task_futures: Vec<JoinHandle<()>>) {
    match future::select_all(task_futures).await.0 {
        Ok(_) => {
            graceful_shutdown();
            vlog::info!("One of the actors finished its run, while it wasn't expected to do it");
        }
        Err(error) => {
            graceful_shutdown();
            vlog::info!(
                "One of the tokio actors unexpectedly finished with error: {:?}",
                error
            );
        }
    }
}

fn graceful_shutdown() {
    let pool = ConnectionPool::new(Some(1), true);
    let host = local_ip().expect("Failed obtaining local IP address");
    let port = ProverConfigs::from_env().non_gpu.assembly_receiver_port;
    let address = SocketAddress {
        host,
        port,
    };
    pool.clone()
        .access_storage_blocking()
        .gpu_prover_queue_dal()
        .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, 0);
}

fn get_ram_per_gpu() -> u64 {
    let device_info = gpu_prover::cuda_bindings::device_info(0).unwrap();
    let ram_in_gb: u64 = device_info.total / (1024 * 1024 * 1024);
    vlog::info!("Detected RAM per GPU: {:?} GB", ram_in_gb);
    ram_in_gb
}

fn get_prover_config_for_machine_type() -> ProverConfig {
    let prover_configs = ProverConfigs::from_env();
    let actual_num_gpus = gpu_prover::cuda_bindings::devices().unwrap() as usize;
    vlog::info!("detected number of gpus: {}", actual_num_gpus);
    let ram_in_gb = get_ram_per_gpu();

    match actual_num_gpus {
        1 => {
            vlog::info!("Detected machine type with 1 GPU and 80GB RAM");
            prover_configs.one_gpu_eighty_gb_mem
        }
        2 => {
            if ram_in_gb > 39 {
                vlog::info!("Detected machine type with 2 GPU and 80GB RAM");
                prover_configs.two_gpu_eighty_gb_mem
            } else {
                vlog::info!("Detected machine type with 2 GPU and 40GB RAM");
                prover_configs.two_gpu_forty_gb_mem
            }
        }
        4 => {
            vlog::info!("Detected machine type with 4 GPU and 80GB RAM");
            prover_configs.four_gpu_eighty_gb_mem
        }
        _ => panic!("actual_num_gpus: {} not supported yet", actual_num_gpus),
    }
}

#[tokio::main]
async fn main() {
    let sentry_guard = vlog::init();
    let config = ZkSyncConfig::from_env();
    let prover_config = get_prover_config_for_machine_type();
    let prometheus_config = PrometheusConfig {
        listener_port: prover_config.prometheus_port,
        ..ApiConfig::from_env().prometheus
    };

    match sentry_guard {
        Some(_) => vlog::info!(
            "Starting Sentry url: {}",
            std::env::var("MISC_SENTRY_URL").unwrap(),
        ),
        None => vlog::info!("No sentry url configured"),
    }
    let region = get_region().await;

    let (stop_signal_sender, mut stop_signal_receiver) = mpsc::channel(256);

    {
        let stop_signal_sender = RefCell::new(stop_signal_sender.clone());
        ctrlc::set_handler(move || {
            let mut sender = stop_signal_sender.borrow_mut();
            block_on(sender.send(true)).expect("Ctrl+C signal send");
        })
        .expect("Error setting Ctrl+C handler");
    }

    zksync_prover_utils::ensure_initial_setup_keys_present(
        &prover_config.initial_setup_key_path,
        &prover_config.key_download_url,
    );
    env::set_var("CRS_FILE", prover_config.initial_setup_key_path.clone());

    let circuit_breaker_checker = CircuitBreakerChecker::new(
        vec![Box::new(VksChecker::new(&config))],
        &config.chain.circuit_breaker,
    );
    circuit_breaker_checker
        .check()
        .await
        .expect("Circuit breaker triggered");
    let (cb_sender, cb_receiver) = futures::channel::oneshot::channel();
    // We don't have a graceful shutdown process for the prover, so `_stop_sender` is unused.
    // Though we still need to create a channel because circuit breaker expects `stop_receiver`.
    let (_stop_sender, stop_receiver) = tokio::sync::watch::channel(false);

    let circuit_ids = ProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(prover_config.specialized_prover_group_id);

    vlog::info!("Starting proof generation for circuits: {:?} in region: {} with group-id: {}", circuit_ids, region, prover_config.specialized_prover_group_id);
    let mut tasks: Vec<JoinHandle<()>> = vec![];

    tasks.push(prometheus_exporter::run_prometheus_exporter(
        prometheus_config,
        true,
    ));
    tasks.push(tokio::spawn(
        circuit_breaker_checker.run(cb_sender, stop_receiver),
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
    let synthesized_circuit_provider =
        SynthesizedCircuitProvider::new(consumer, ConnectionPool::new(Some(1), true), address);
    vlog::info!("local IP address is: {:?}", local_ip);

    tasks.push(tokio::task::spawn(incoming_socket_listener(
        local_ip,
        prover_config.assembly_receiver_port,
        prover_config.assembly_receiver_poll_time_in_millis,
        producer,
        ConnectionPool::new(Some(1), true),
        prover_config.specialized_prover_group_id,
        region
    )));

    let artifact_provider = ProverArtifactProvider {};
    let prover_job_reporter = ProverReporter {
        pool: ConnectionPool::new(Some(1), true),
        config: prover_config.clone(),
        processed_by: env::var("POD_NAME").unwrap_or("Unknown".to_string()),
    };

    let params: ProverParams = prover_config.clone().into();

    tasks.push(tokio::task::spawn_blocking(move || {
        run_prover_with_remote_synthesizer::<_, _, _, _>(
            synthesized_circuit_provider,
            artifact_provider,
            prover_job_reporter,
            circuit_ids,
            params,
        )
    }));

    tokio::select! {
        _ = async { wait_for_tasks(tasks).await } => {},
        _ = async { stop_signal_receiver.next().await } => {
            vlog::info!("Stop signal received, shutting down");
        },
        error = async { cb_receiver.await } => {
            if let Ok(error_msg) = error {
                vlog::warn!("Circuit breaker received, shutting down. Reason: {}", error_msg);
            }
        },
    };
}
