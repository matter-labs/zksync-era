use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};

// use tokio::fs::File;
use anyhow::Context;
use clap::Parser;
use shivini::ProverContext;
use zksync_circuit_prover_service::{
    gpu_circuit_prover::GpuCircuitProverExecutor,
    types::{
        circuit::Circuit, circuit_prover_payload::GpuCircuitProverPayload,
        witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
        witness_vector_generator_payload::WitnessVectorGeneratorPayload,
    },
    witness_vector_generator::WitnessVectorGeneratorExecutor,
};
use zksync_config::{configs::ObservabilityConfig, ObjectStoreConfig};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    CircuitWrapper, ProverServiceDataKey,
};
use zksync_prover_job_processor::Executor;
use zksync_prover_keystore::{
    keystore::{Keystore, ProverServiceDataType},
    GoldilocksGpuProverSetupData,
};
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::FriProverJobMetadata, L1BatchNumber,
};

pub type FinalizationHintsCache = HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>;

async fn prepare_wvg(
    object_store: Arc<dyn ObjectStore>,
    finalization_hints_cache: FinalizationHintsCache,
    metadata: FriProverJobMetadata,
) -> anyhow::Result<WitnessVectorGeneratorExecutionOutput> {
    let start_time = Instant::now();
    tracing::info!("Started picking witness vector generator job");
    let circuit_wrapper = object_store
        .get(metadata.into())
        .await
        .context("failed to get circuit_wrapper from object store")?;
    let circuit = match circuit_wrapper {
        CircuitWrapper::Base(circuit) => Circuit::Base(circuit),
        _ => panic!("Unsupported circuit"),
    };
    tracing::info!("Circuit loaded");

    let key = ProverServiceDataKey {
        circuit_id: metadata.circuit_id,
        stage: metadata.aggregation_round.into(),
    }
    .crypto_setup_key();
    let finalization_hints = finalization_hints_cache
        .get(&key)
        .context("failed to retrieve finalization key from cache")?
        .clone();

    tracing::info!(
            "Finished picking witness vector generator job {}, on batch {}, for circuit {}, at round {} in {:?}",
            metadata.id,
            metadata.block_number,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );

    let executor = WitnessVectorGeneratorExecutor {};
    let wvg = executor.execute(
        WitnessVectorGeneratorPayload {
            circuit,
            finalization_hints,
        },
        metadata,
    )?;

    // Dump witness_vector into file.
    //let mut file = File::create("witness_vector.bin").await?;
    //let buf = bincode::serialize(&wvg.witness_vector)?;
    //file.write_all(&buf[..]).await?;
    Ok(wvg)
}

fn get_setup_data_path() -> PathBuf {
    "/".into() // TODO: Maybe change to /usr/src/setup-data
}

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Path to file configuration
    #[arg(short = 'd', long, default_value = get_setup_data_path().into_os_string())]
    pub(crate) setup_data_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start_time = Instant::now();
    let opt = Cli::parse();

    let observability_config = ObservabilityConfig {
        sentry_url: None,
        sentry_environment: None,
        opentelemetry: None,
        log_format: "json".to_string(),
        log_directives: None,
    };
    let _observability_guard = observability_config
        .install()
        .context("failed to install observability")?;

    let object_store_config = ObjectStoreConfig {
        mode: zksync_config::configs::object_store::ObjectStoreMode::FileBacked {
            file_backed_base_path: opt.setup_data_path.display().to_string(),
        },
        max_retries: 1,
        local_mirror_path: None,
    };
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .context("failed to create object store")?;

    let keystore = Keystore::locate().with_setup_path(Some(opt.setup_data_path));

    tracing::info!("Loading finalization hints from disk...");
    let finalization_hints = keystore
        .load_all_finalization_hints_mapping()
        .await
        .context("failed to load finalization hints mapping")?;

    // Expected file prover_jobs_fri/10330_48_1_BasicCircuits_0.bin.
    let metadata = FriProverJobMetadata {
        id: 1,
        block_number: L1BatchNumber(10330),
        circuit_id: 1,
        aggregation_round: AggregationRound::BasicCircuits,
        sequence_number: 48,
        depth: 0,
        is_node_final_proof: false,
        pick_time: Instant::now(),
    };
    let WitnessVectorGeneratorExecutionOutput {
        circuit,
        witness_vector,
    } = prepare_wvg(object_store, finalization_hints, metadata).await?;

    tracing::info!("Loading setup data from disk...");
    let setup_data = keystore
        .load_a_key_mapping::<GoldilocksGpuProverSetupData>(
            ProverServiceDataKey::new_basic(1), // Only BasicCircuits #1.
            ProverServiceDataType::SetupData,
        )
        .await
        .context("failed to load setup key mapping")?;

    let prover_context =
        ProverContext::create().context("failed initializing gpu prover context")?;
    let prover = GpuCircuitProverExecutor::new(prover_context);
    let _ = prover.execute(
        GpuCircuitProverPayload {
            circuit,
            witness_vector,
            setup_data,
        },
        metadata,
    )?;

    tracing::info!("Finished generating proof in {:?}", start_time.elapsed());
    Ok(())
}
