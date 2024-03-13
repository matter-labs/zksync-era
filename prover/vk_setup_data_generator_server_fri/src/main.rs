//! Tool to generate different types of keys used by the proving system.
//!
//! It can generate verification keys, setup keys, and also commitments.
use std::collections::HashMap;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use commitment_generator::read_and_update_contract_toml;
use tracing::level_filters::LevelFilter;
use zkevm_test_harness::{
    compute_setups::{
        generate_base_layer_vks_and_proofs, generate_eip4844_vks,
        generate_recursive_layer_vks_and_proofs,
    },
    data_source::{in_memory_data_source::InMemoryDataSource, SetupDataSource},
    proof_wrapper_utils::{
        check_trusted_setup_file_existace, get_wrapper_setup_and_vk_from_scheduler_vk,
        WrapperConfig,
    },
};
use zksync_prover_fri_types::{
    circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
    ProverServiceDataKey,
};
use zksync_vk_setup_data_server_fri::{
    commitment_utils::generate_commitments,
    keystore::Keystore,
    setup_data_generator::{CPUSetupDataGenerator, GPUSetupDataGenerator, SetupDataGenerator},
};

mod commitment_generator;

#[cfg(test)]
mod tests;

fn generate_vks(keystore: &Keystore) -> anyhow::Result<()> {
    // Start by checking the trusted setup existence.
    // This is used at the last step, but we want to fail early if user didn't configure everything
    // correctly.
    check_trusted_setup_file_existace();

    let mut in_memory_source = InMemoryDataSource::new();
    tracing::info!("Generating verification keys for Base layer.");
    generate_base_layer_vks_and_proofs(&mut in_memory_source)
        .map_err(|err| anyhow::anyhow!("Failed generating base vk's: {err}"))?;

    // This must happen before we start generating recursive layer.
    tracing::info!("Generating 4844 base layer.");
    generate_eip4844_vks(&mut in_memory_source)
        .map_err(|err| anyhow::anyhow!("Failed generating 4844 vk's: {err}"))?;

    tracing::info!("Generating verification keys for Recursive layer.");
    generate_recursive_layer_vks_and_proofs(&mut in_memory_source)
        .map_err(|err| anyhow::anyhow!("Failed generating recursive vk's: {err}"))?;

    tracing::info!("Saving keys & hints");

    keystore.save_keys_from_data_source(&in_memory_source)?;

    // Generate snark VK
    let scheduler_vk = in_memory_source
        .get_recursion_layer_vk(ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8)
        .map_err(|err| anyhow::anyhow!("Failed to get scheduler vk: {err}"))?;
    tracing::info!("Generating verification keys for snark wrapper.");

    // Compression mode is 1
    let config = WrapperConfig::new(1);

    let (_, vk) = get_wrapper_setup_and_vk_from_scheduler_vk(scheduler_vk, config);
    keystore
        .save_snark_verification_key(vk)
        .context("save_snark_vk")?;

    // Let's also update the commitments file.
    keystore.save_commitments(&generate_commitments(keystore)?)
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
    /// EIP 4844 circuit
    Eip4844,
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
    },
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

fn keystore_from_optional_path(path: Option<String>, setup_path: Option<String>) -> Keystore {
    if let Some(path) = path {
        return Keystore::new_with_optional_setup_path(path, setup_path);
    }
    if setup_path.is_some() {
        panic!("--setup_path must not be set when --path is not set");
    }
    Keystore::default()
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
        CircuitSelector::Eip4844 => ProverServiceDataKey::eip4844(),
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
        Command::GenerateVerificationKeys { path } => {
            let keystore = keystore_from_optional_path(path, None);
            tracing::info!(
                "Generating verification keys and storing them inside {:?}",
                keystore.get_base_path()
            );
            generate_vks(&keystore).context("generate_vks()")
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
    }
}
