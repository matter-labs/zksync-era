#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::{fs, path::PathBuf, sync::Arc, time::Instant};

use anyhow::Context;
use clap::Parser;
use proof_compression_gpu::{run_proof_chain, CompressorBlobStorage, SnarkWrapper};
use zksync_config::configs::FriProofCompressorConfig;
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof,
        zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
    },
    AuxOutputWitnessWrapper, PROVER_PROTOCOL_SEMANTIC_VERSION,
};
use zksync_prover_interface::outputs::{
    FflonkL1BatchProofForL1, L1BatchProofForL1, PlonkL1BatchProofForL1,
};
use zksync_prover_keystore::{compressor::load_all_resources, keystore::Keystore};

/// CLI arguments for the proof compressor file runner
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Path to the input file containing the scheduler proof (bincode format)
    /// Default: ./proofs_fri_proof_21680346_123.bin
    #[arg(long, default_value = "proofs_fri_proof_21680346_123.bin")]
    scheduler_proof: PathBuf,

    /// Path to the input file containing the aux output witness (bincode format)
    /// Default: ./scheduler_witness_jobs_fri_aux_output_witness_22951_123.bin
    #[arg(long, default_value = "scheduler_witness_jobs_fri_aux_output_witness_22951_123.bin")]
    aux_witness: PathBuf,

    /// Path to the output file for the compressed proof (bincode format)
    /// Default: ./compressed_proof.bin
    #[arg(long, default_value = "compressed_proof.bin")]
    output: PathBuf,

    /// Use FFLONK wrapper (default: FFLONK)
    #[arg(long, default_value = "true")]
    fflonk: bool,

    /// Path to setup data directory
    #[arg(long)]
    setup_data_path: PathBuf,

    /// Path to universal setup (CRS) file
    #[arg(long)]
    universal_setup_path: PathBuf,
}

fn aux_output_witness_to_array(
    aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
) -> [[u8; 32]; 4] {
    let mut array: [[u8; 32]; 4] = [[0; 32]; 4];

    for i in 0..32 {
        array[0][i] = aux_output_witness.l1_messages_linear_hash[i];
        array[1][i] = aux_output_witness.rollup_state_diff_for_compression[i];
        array[2][i] = aux_output_witness.bootloader_heap_initial_content[i];
        array[3][i] = aux_output_witness.events_queue_state[i];
    }
    array
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();

    tracing::info!("Starting proof compressor file runner");
    tracing::info!("Scheduler proof: {:?}", args.scheduler_proof);
    tracing::info!("Aux witness: {:?}", args.aux_witness);
    tracing::info!("Output: {:?}", args.output);
    tracing::info!("FFLONK mode: {}", args.fflonk);

    // Read scheduler proof from file
    let start_time = Instant::now();
    tracing::info!("Reading scheduler proof from file...");
    let scheduler_proof_bytes = fs::read(&args.scheduler_proof)
        .with_context(|| format!("Failed to read scheduler proof from {:?}", args.scheduler_proof))?;
    let scheduler_proof: ZkSyncRecursionLayerProof = bincode::deserialize(&scheduler_proof_bytes)
        .context("Failed to deserialize scheduler proof")?;
    tracing::info!(
        "Scheduler proof loaded in {:?}, size: {} bytes",
        start_time.elapsed(),
        scheduler_proof_bytes.len()
    );

    // Read aux output witness from file
    let start_time = Instant::now();
    tracing::info!("Reading aux output witness from file...");
    let aux_witness_bytes = fs::read(&args.aux_witness)
        .with_context(|| format!("Failed to read aux witness from {:?}", args.aux_witness))?;
    let aux_output_witness_wrapper: AuxOutputWitnessWrapper =
        bincode::deserialize(&aux_witness_bytes).context("Failed to deserialize aux witness")?;
    tracing::info!(
        "Aux witness loaded in {:?}, size: {} bytes",
        start_time.elapsed(),
        aux_witness_bytes.len()
    );

    // Setup CRS keys
    tracing::info!("Setting up CRS keys...");
    std::env::set_var("COMPACT_CRS_FILE", &args.universal_setup_path);

    // Load keystore and setup data
    tracing::info!("Loading keystore from {:?}...", args.setup_data_path);
    let keystore = Keystore::locate().with_setup_path(Some(args.setup_data_path));
    let keystore = Arc::new(keystore);
    load_all_resources(&keystore, args.fflonk);

    // Determine wrapper mode
    let snark_wrapper_mode = if args.fflonk {
        SnarkWrapper::Fflonk
    } else {
        SnarkWrapper::Plonk
    };

    // Run proof compression
    tracing::info!("Starting proof compression...");
    let start_time = Instant::now();
    let proof_wrapper = run_proof_chain(snark_wrapper_mode, keystore.clone(), scheduler_proof)?;

    let aggregation_result_coords =
        aux_output_witness_to_array(aux_output_witness_wrapper.0);

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;
    let l1_batch_proof: L1BatchProofForL1 = match proof_wrapper {
        proof_compression_gpu::SnarkWrapperProof::Plonk(proof) => {
            L1BatchProofForL1::new_plonk(PlonkL1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof: proof,
                protocol_version,
            })
        }
        proof_compression_gpu::SnarkWrapperProof::Fflonk(proof) => {
            L1BatchProofForL1::new_fflonk(FflonkL1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof: proof,
                protocol_version,
            })
        }
    };

    tracing::info!(
        "Proof compression finished in {:?}",
        start_time.elapsed()
    );

    // Save result to file
    tracing::info!("Saving compressed proof to {:?}...", args.output);
    let output_bytes =
        bincode::serialize(&l1_batch_proof).context("Failed to serialize output proof")?;
    fs::write(&args.output, &output_bytes)
        .with_context(|| format!("Failed to write output to {:?}", args.output))?;

    tracing::info!(
        "Compressed proof saved successfully, size: {} bytes",
        output_bytes.len()
    );
    tracing::info!("Done!");

    Ok(())
}

