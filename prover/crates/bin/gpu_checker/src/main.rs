use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::Parser;
use once_cell::sync::Lazy;
use regex::Regex;
use shivini::ProverContext;
use tokio::fs;
use zksync_circuit_prover_service::{
    gpu_circuit_prover::GpuCircuitProverExecutor,
    types::{
        circuit_prover_payload::GpuCircuitProverPayload, setup_data::GoldilocksGpuProverSetupData,
        witness_vector_generator_payload::WitnessVectorGeneratorPayload,
    },
    witness_vector_generator::WitnessVectorGeneratorExecutor,
};
use zksync_config::{configs::ObservabilityConfig, ObjectStoreConfig};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::{
        cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
    },
    ProverServiceDataKey,
};
use zksync_prover_job_processor::Executor;
use zksync_prover_keystore::keystore::{Keystore, ProverServiceDataType};
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::FriProverJobMetadata, L1BatchId, L1BatchNumber,
    L2ChainId,
};

async fn create_witness_vector(
    metadata: FriProverJobMetadata,
    object_store: Arc<dyn ObjectStore>,
    keystore: Keystore,
) -> anyhow::Result<()> {
    let start_time = Instant::now();

    tracing::info!("Started picking witness vector generator job");
    let circuit_wrapper = object_store
        .get(metadata.into())
        .await
        .context("failed to get circuit_wrapper from object store")?;
    tracing::info!("Circuit loaded");

    tracing::info!("Loading finalization hints from disk...");
    let finalization_hints_cache = keystore
        .load_all_finalization_hints_mapping()
        .await
        .context("failed to load finalization hints mapping")?;
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
            metadata.batch_id.batch_number().0,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );

    let executor = WitnessVectorGeneratorExecutor {};
    let wvg = executor.execute(
        WitnessVectorGeneratorPayload {
            circuit_wrapper,
            finalization_hints,
        },
        metadata,
    )?;

    // Dump witness_vector into file.
    let buf = bincode::serialize(&wvg.witness_vector)?;
    fs::write(witness_vector_filename(metadata), buf).await?;

    Ok(())
}

async fn read_witness_vector(path: PathBuf) -> anyhow::Result<WitnessVec<GoldilocksField>> {
    let buf = fs::read(path).await?;
    Ok(bincode::deserialize(&buf[..])?)
}

async fn run_prover(
    metadata: FriProverJobMetadata,
    object_store: Arc<dyn ObjectStore>,
    keystore: Keystore,
    witness_vector: WitnessVec<GoldilocksField>,
) -> anyhow::Result<()> {
    // Run GPU prover
    let start_time = Instant::now();
    tracing::info!("Loading setup data from disk...");

    tracing::info!("Loading citcuit");
    let circuit_wrapper = object_store
        .get(metadata.into())
        .await
        .context("failed to get circuit_wrapper from object store")?;
    tracing::info!("Circuit loaded");

    let setup_data = keystore
        .load_single_key_mapping::<GoldilocksGpuProverSetupData>(
            ProverServiceDataKey::new_basic(metadata.circuit_id), // Load only needed circuit.
            ProverServiceDataType::SetupData,
        )
        .await
        .context("failed to load setup key mapping")?;

    let prover_context =
        ProverContext::create().context("failed initializing gpu prover context")?;
    let prover = GpuCircuitProverExecutor::new(prover_context);
    let _ = prover.execute(
        GpuCircuitProverPayload {
            circuit_wrapper,
            witness_vector,
            setup_data,
        },
        metadata,
    )?;

    tracing::info!("Finished generating proof in {:?}", start_time.elapsed());
    Ok(())
}

// Re to extract metadata from the file name. Only BasicCircuits are supported.
static CIRCUIT_FILE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?<block>\d+)_(?<sequence>\d+)_(?<circuit>\d+)_BasicCircuits_(?<depth>\d+)\.")
        .unwrap()
});

fn get_metadata(path: &Path) -> anyhow::Result<FriProverJobMetadata> {
    let file = path.file_name().context("missing file name")?;
    let caps = CIRCUIT_FILE_RE
        .captures(file.to_str().context("invalid file name")?)
        .context(format!(
            "wrong file {file:?}, note only BasicCircuits are supported!"
        ))?;

    // Expected file like prover_jobs_fri/10330_48_1_BasicCircuits_0.bin.
    Ok(FriProverJobMetadata {
        id: 1,
        batch_id: L1BatchId::new(L2ChainId::zero(), L1BatchNumber(caps["block"].parse()?)),
        circuit_id: caps["circuit"].parse()?,
        aggregation_round: AggregationRound::BasicCircuits,
        sequence_number: caps["sequence"].parse()?,
        depth: caps["depth"].parse()?,
        is_node_final_proof: false,
        pick_time: Instant::now(),
        batch_sealed_at: DateTime::<Utc>::default(),
    })
}

fn witness_vector_filename(metadata: FriProverJobMetadata) -> String {
    let FriProverJobMetadata {
        id: _,
        batch_id,
        sequence_number,
        circuit_id,
        aggregation_round,
        depth,
        is_node_final_proof: _,
        pick_time: _,
        batch_sealed_at: _,
    } = metadata;
    format!("{block_number}_{sequence_number}_{circuit_id}_{aggregation_round:?}_{depth}.witness_vector", block_number = batch_id.batch_number().0)
}

fn get_setup_data_path() -> PathBuf {
    "/".into()
}

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Path to file configuration
    #[arg(short = 'o', long, default_value = get_setup_data_path().into_os_string())]
    pub(crate) object_store_path: PathBuf,

    /// Path to file configuration
    #[arg(short = 'k', long, default_value = get_setup_data_path().into_os_string())]
    pub(crate) keystore_path: PathBuf,

    // Circuit file name, eg: prover_jobs_fri/10330_48_1_BasicCircuits_0.bin
    #[arg(short = 'c', long)]
    pub(crate) circuit_file: Option<PathBuf>,

    // Witness Vector file name, eg: 10330_48_1_BasicCircuits_0.witness_vector
    #[arg(short = 'w', long)]
    pub(crate) witness_vector_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let observability_config = ObservabilityConfig {
        sentry: None,
        opentelemetry: None,
        log_format: "json".to_string(),
        ..ObservabilityConfig::default()
    };
    let _observability_guard = observability_config
        .install()
        .context("failed to install observability")?;

    let object_store_config = ObjectStoreConfig {
        mode: zksync_config::configs::object_store::ObjectStoreMode::FileBacked {
            file_backed_base_path: opt.object_store_path,
        },
        max_retries: 1,
        local_mirror_path: None,
    };
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .context("failed to create object store")?;

    let keystore = Keystore::locate().with_setup_path(Some(opt.keystore_path));

    if let Some(circuit_file) = opt.circuit_file {
        let metadata = get_metadata(&circuit_file)?;
        create_witness_vector(metadata, object_store.clone(), keystore.clone()).await?;
    }

    if let Some(witness_vector_file) = opt.witness_vector_file {
        let metadata = get_metadata(&witness_vector_file)?;
        let wvg = read_witness_vector(witness_vector_file).await?;
        run_prover(metadata, object_store, keystore, wvg).await?;
    }

    Ok(())
}
