#![feature(generic_const_exprs)]

use std::time::Duration;

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::ConnectionPool;
use structopt::StructOpt;
use tokio::sync::{oneshot, watch};
use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, FriProverConfig, FriWitnessVectorGeneratorConfig,
    ObservabilityConfig, PostgresConfig,
};
use zksync_env_config::{object_store::ProverObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_fri_types::{
    circuit_definitions::boojum::field::goldilocks::GoldilocksField, keys::FriCircuitKey,
    CircuitWrapper, ProverServiceDataKey,
};
use zksync_prover_fri_utils::{get_all_circuit_id_round_tuples_for, region_fetcher::get_zone};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{L1BatchNumber, ProtocolVersionId};
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vk_setup_data_server_fri::keystore::Keystore;

use crate::generator::WitnessVectorGenerator;

mod generator;
mod metrics;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "zksync_witness_vector_generator",
    about = "Tool for generating witness vectors for circuits"
)]
struct Opt {
    /// Number of times `witness_vector_generator` should be run.
    #[structopt(short = "n", long = "n_iterations")]
    number_of_iterations: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let object_store_config =
        ProverObjectStoreConfig::from_env().context("ProverObjectStoreConfig::from_env()")?;
    let blob_store = ObjectStoreFactory::new(object_store_config.0)
        .create_store()
        .await;

    let circuit_id = 1;
    let aggregation_round = 0.into();

    let circuit_key = FriCircuitKey {
        block_number: L1BatchNumber(8853),
        sequence_number: 1670,
        circuit_id,
        aggregation_round,
        depth: 0,
    };

    let input: CircuitWrapper = blob_store
        .get(circuit_key)
        .await
        .unwrap_or_else(|err| panic!("{err:?}"));

    let keystore = Keystore::default();

    let setup_data_key = ProverServiceDataKey {
        circuit_id,
        round: aggregation_round,
    };

    let finalization_hints = keystore
        .load_finalization_hints(setup_data_key)
        .context("get_finalization_hints()")?;

    let _ = match input {
        CircuitWrapper::Base(base_circuit) => {
            base_circuit.synthesis::<GoldilocksField>(&finalization_hints)
        }
        _ => panic!("unexpected"),
    };
    println!("got CS");
    Ok(())
}
