#![feature(allocator_api, generic_const_exprs)]
#![allow(incomplete_features)]

//! Tool to generate different types of keys used by the proving system.
//!
//! It can generate verification keys, setup keys, and also commitments.
use std::{collections::HashMap, path::PathBuf};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use commitment_generator::read_and_update_contract_toml;
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(feature = "gpu")]
use proof_compression_gpu::{
    precompute_proof_chain_with_fflonk, precompute_proof_chain_with_plonk,
};
use tracing::level_filters::LevelFilter;
use zkevm_test_harness::{
    compute_setups::{
        basic_vk_count, generate_base_layer_vks, generate_recursive_layer_vks,
        recursive_layer_vk_count,
    },
    data_source::in_memory_data_source::InMemoryDataSource,
};
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_prover_keystore::{
    keystore::Keystore,
    setup_data_generator::{CPUSetupDataGenerator, GPUSetupDataGenerator, SetupDataGenerator},
};

mod commitment_generator;
mod vk_commitment_helper;

#[cfg(test)]
mod tests;

/// Generates new verification keys, and stores them in `keystore`.
/// Jobs describe how many generators can run in parallel (each one is around 30 GB).
/// If quiet is true, it doesn't display any progress bar.
fn generate_vks(keystore: &Keystore, jobs: usize, quiet: bool) -> anyhow::Result<()> {
    let progress_bar = if quiet {
        None
    } else {
        let count = basic_vk_count() + recursive_layer_vk_count() + 2;
        let progress_bar = ProgressBar::new(count as u64);
        progress_bar.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} ({eta})")
            .progress_chars("#>-"));
        Some(progress_bar)
    };

    let pb = std::sync::Arc::new(std::sync::Mutex::new(progress_bar));

    let mut in_memory_source = InMemoryDataSource::new();
    tracing::info!("Generating verification keys for Base layer.");

    generate_base_layer_vks(&mut in_memory_source, Some(jobs), || {
        if let Some(p) = pb.lock().unwrap().as_ref() {
            p.inc(1)
        }
    })
    .map_err(|err| anyhow::anyhow!("Failed generating base vk's: {err}"))?;

    tracing::info!("Generating verification keys for Recursive layer.");
    generate_recursive_layer_vks(&mut in_memory_source, Some(jobs), || {
        if let Some(p) = pb.lock().unwrap().as_ref() {
            p.inc(1)
        }
    })
    .map_err(|err| anyhow::anyhow!("Failed generating recursive vk's: {err}"))?;

    keystore.save_keys_from_data_source(&in_memory_source)
}

#[derive(Debug, Parser)]
#[command(
    author = "Matter Labs",
    version,
    about = "Key generation tool. See https://github.com/matter-labs/zksync-era/blob/main/docs/guides/advanced/prover_keys.md for details.",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum CircuitSelector {
    /// Select all circuits
    All,
    /// Select circuits from recursive group (leaf, node, scheduler)
    Recursive,
    /// Select circuits from basic group.
    Basic,
}

#[derive(Debug, Parser)]
struct GeneratorOptions {
    circuits_type: CircuitSelector,

    /// Specify for which circuit to generate the keys.
    #[arg(long)]
    numeric_circuit: Option<u8>,

    /// If true, then setup keys are not written and only md5 sum is printed.
    #[arg(long, default_value = "false")]
    dry_run: bool,

    /// If true, then generate the setup key, only if it is missing.
    // Warning: this doesn't check the correctness of the existing file.
    #[arg(long, default_value = "false")]
    recompute_if_missing: bool,

    #[arg(long)]
    path: Option<String>,

    #[arg(long)]
    setup_path: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generates verification keys (and finalization hints) for all the basic & leaf circuits.
    /// Used for verification.
    #[command(name = "generate-vk")]
    GenerateVerificationKeys {
        #[arg(long)]
        path: Option<String>,
        /// Number of generators to run in parallel - each one consumes around 30 GB of RAM.
        #[arg(short, long, default_value_t = 1)]
        jobs: usize,
        /// If true - disables progress bar.
        #[arg(long)]
        quiet: bool,
    },
    #[command(name = "generate-compressor-data")]
    GenerateCompressorPrecomputations,
    #[command(name = "generate-crs")]
    GenerateCompactCrs,
    /// Generates setup keys (used by the CPU prover).
    #[command(name = "generate-sk")]
    GenerateSetupKeys {
        #[command(flatten)]
        options: GeneratorOptions,
    },
    /// Generates setup keys (used by the GPU prover).
    #[command(name = "generate-sk-gpu")]
    GenerateGPUSetupKeys {
        #[command(flatten)]
        options: GeneratorOptions,
    },
    /// Generates and updates the commitments - used by the verification contracts.
    #[command(name = "update-commitments")]
    UpdateCommitments {
        #[arg(long)]
        dryrun: bool,
        #[arg(long)]
        path: Option<String>,
    },
}

fn print_stats(digests: HashMap<String, String>) -> anyhow::Result<()> {
    let mut keys: Vec<&String> = digests.keys().collect();
    keys.sort();
    // Iterate over the sorted keys
    for key in keys {
        if let Some(value) = digests.get(key) {
            tracing::info!("{key}: {value}");
        }
    }
    Ok(())
}

fn keystore_from_optional_path(path: Option<String>, setup_data_path: Option<String>) -> Keystore {
    if let Some(path) = path {
        return Keystore::new(path.into()).with_setup_path(setup_data_path.map(PathBuf::from));
    }
    if setup_data_path.is_some() {
        panic!("--setup_path must not be set when --path is not set");
    }
    Keystore::locate()
}

fn generate_setup_keys(
    generator: &dyn SetupDataGenerator,
    options: &GeneratorOptions,
) -> anyhow::Result<()> {
    let circuit_type = match options.circuits_type {
        CircuitSelector::All => {
            let digests = generator.generate_all(options.dry_run, options.recompute_if_missing)?;
            tracing::info!("Setup keys md5(s):");
            print_stats(digests)?;
            return Ok(());
        }
        CircuitSelector::Recursive => ProverServiceDataKey::new_recursive(
            options
                .numeric_circuit
                .expect("--numeric-circuit must be provided"),
        ),
        CircuitSelector::Basic => ProverServiceDataKey::new_basic(
            options
                .numeric_circuit
                .expect("--numeric-circuit must be provided"),
        ),
    };

    let digest = generator
        .generate_and_write_setup_data(circuit_type, options.dry_run, options.recompute_if_missing)
        .context("generate_setup_data()")?;
    tracing::info!("digest: {:?}", digest);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let opt = Cli::parse();

    match opt.command {
        Command::GenerateVerificationKeys { path, jobs, quiet } => {
            let keystore = keystore_from_optional_path(path, None);
            tracing::info!(
                "Generating verification keys and storing them inside {:?}",
                keystore.get_base_path()
            );
            generate_vks(&keystore, jobs, quiet).context("generate_vks()")
        }
        Command::UpdateCommitments { dryrun, path } => {
            let keystore = keystore_from_optional_path(path, None);

            read_and_update_contract_toml(&keystore, dryrun)
        }
        Command::GenerateSetupKeys { options } => {
            let generator = CPUSetupDataGenerator {
                keystore: keystore_from_optional_path(
                    options.path.clone(),
                    options.setup_path.clone(),
                ),
            };
            generate_setup_keys(&generator, &options)
        }
        Command::GenerateGPUSetupKeys { options } => {
            let generator = GPUSetupDataGenerator {
                keystore: keystore_from_optional_path(
                    options.path.clone(),
                    options.setup_path.clone(),
                ),
            };
            generate_setup_keys(&generator, &options)
        }
        Command::GenerateCompressorPrecomputations => {
            #[cfg(not(feature = "gpu"))]
            {
                anyhow::bail!("Must compile with --gpu feature to use this option.")
            }
            #[cfg(feature = "gpu")]
            {
                let keystore = Keystore::locate();
                precompute_proof_chain_with_plonk(std::sync::Arc::new(keystore.clone()));
                precompute_proof_chain_with_fflonk(std::sync::Arc::new(keystore.clone()));

                let commitments = keystore.generate_commitments()?;
                keystore.save_commitments(&commitments)
            }
        }
        Command::GenerateCompactCrs => {
            #[cfg(not(feature = "gpu"))]
            {
                anyhow::bail!("Must compile with --gpu feature to use this option.")
            }
            #[cfg(feature = "gpu")]
            {
                let keystore = Keystore::locate();

                if std::env::var("COMPACT_CRS_FILE").is_err() {
                    return Err(anyhow::anyhow!("COMPACT_CRS_FILE env variable is not set"));
                }

                if std::env::var("IGNITION_TRANSCRIPT_PATH").is_err() {
                    return Err(anyhow::anyhow!(
                        "IGNITION_TRANSCRIPT_PATH env variable is not set"
                    ));
                }

                Ok(proof_compression_gpu::create_compact_raw_crs(
                    keystore.write_compact_raw_crs(),
                ))
            }
        }
    }
}
