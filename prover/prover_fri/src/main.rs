#![feature(generic_const_exprs)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use zksync_vk_setup_data_server_fri::{
    get_setup_data_for_circuit_type, GoldilocksProverSetupData, ProverServiceDataKey,
};

use zksync_config::configs::fri_prover_group::{CircuitIdRoundTuple, FriProverGroupConfig};
use zksync_config::configs::{FriProverConfig, PrometheusConfig};
use zksync_config::{ApiConfig, ObjectStoreConfig};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::wait_for_tasks;

use crate::prover_job_processor::{Prover, SetupLoadMode};

#[cfg(feature = "gpu")]
use zksync_vk_setup_data_server_fri::GoldilocksGpuProverSetupData;

mod prover_job_processor;

#[tokio::main]
async fn main() {
    vlog::init();
    let sentry_guard = vlog::init_sentry();
    let prover_config = FriProverConfig::from_env();
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

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let blob_store = ObjectStoreFactory::from_env().create_store().await;
    let public_blob_store = ObjectStoreFactory::new(ObjectStoreConfig::public_from_env())
        .create_store()
        .await;

    vlog::info!("Starting FRI proof generation");
    let pool = ConnectionPool::new(None, DbVariant::Prover).await;
    let circuit_ids_for_round_to_be_proven = FriProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(prover_config.specialized_group_id)
        .unwrap_or(vec![]);

    let setup_load_mode = build_prover_setup_load_mode_using_config(&prover_config);
    let prover = Prover::new(
        blob_store,
        public_blob_store,
        FriProverConfig::from_env(),
        pool,
        setup_load_mode,
        circuit_ids_for_round_to_be_proven,
    );
    let tasks = vec![
        prometheus_exporter::run_prometheus_exporter(prometheus_config.listener_port, None),
        tokio::spawn(prover.run(stop_receiver, None)),
    ];

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            vlog::info!("Stop signal received, shutting down");
        },
    }

    stop_sender.send(true).ok();
}

fn build_prover_setup_load_mode_using_config(config: &FriProverConfig) -> SetupLoadMode {
    match config.setup_load_mode {
        zksync_config::configs::fri_prover::SetupLoadMode::FromDisk => SetupLoadMode::FromDisk,
        zksync_config::configs::fri_prover::SetupLoadMode::FromMemory => {
            let cache = load_setup_data_cache(config.specialized_group_id);
            SetupLoadMode::FromMemory(cache)
        }
    }
}

#[cfg(not(feature = "gpu"))]
fn load_setup_data_cache(
    specialized_group_id: u8,
) -> HashMap<ProverServiceDataKey, Arc<GoldilocksProverSetupData>> {
    vlog::info!(
        "Loading setup data cache for group {}",
        specialized_group_id
    );
    let prover_setup_metadata_list = FriProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(specialized_group_id)
        .expect(
            "At least one circuit should be configured for group when running in FromMemory mode",
        );
    vlog::info!(
        "for group {} configured setup metadata are {:?}",
        specialized_group_id,
        prover_setup_metadata_list
    );
    let mut cache = HashMap::new();
    for prover_setup_metadata in prover_setup_metadata_list {
        let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
        let setup_data = get_setup_data_for_circuit_type(key.clone());
        cache.insert(key, Arc::new(setup_data));
    }
    cache
}

#[cfg(feature = "gpu")]
fn load_setup_data_cache(
    specialized_group_id: u8,
) -> HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>> {
    vlog::info!(
        "Loading GPU setup data cache for group {}",
        specialized_group_id
    );
    let prover_setup_metadata_list = FriProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(specialized_group_id)
        .expect(
            "At least one circuit should be configured for group when running in FromMemory mode",
        );
    vlog::info!(
        "for group {} configured setup metadata are {:?}",
        specialized_group_id,
        prover_setup_metadata_list
    );
    let mut cache = HashMap::new();
    for prover_setup_metadata in prover_setup_metadata_list {
        let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
        let setup_data = get_setup_data_for_circuit_type(key.clone());
        cache.insert(key, Arc::new(setup_data));
    }
    cache
}
fn setup_metadata_to_setup_data_key(setup_metadata: &CircuitIdRoundTuple) -> ProverServiceDataKey {
    ProverServiceDataKey {
        circuit_id: setup_metadata.circuit_id,
        round: setup_metadata.aggregation_round.into(),
    }
}
