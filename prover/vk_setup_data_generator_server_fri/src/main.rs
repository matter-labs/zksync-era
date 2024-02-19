use std::collections::HashMap;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use commitment_generator::read_and_update_contract_toml;
use tracing::level_filters::LevelFilter;
use zkevm_test_harness::{
    compute_setups::{generate_base_layer_vks_and_proofs, generate_recursive_layer_vks_and_proofs},
    data_source::{in_memory_data_source::InMemoryDataSource, SetupDataSource},
    proof_wrapper_utils::{
        check_trusted_setup_file_existace, get_wrapper_setup_and_vk_from_scheduler_vk,
        WrapperConfig,
    },
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_vk_setup_data_server_fri::{
    keystore::Keystore,
    setup_data_generator::{
        generate_all_cpu_setup_data, generate_all_gpu_setup_data, generate_cpu_setup_data,
        generate_gpu_setup_data,
    },
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
        .context("save_snark_vk")
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

#[derive(Debug, Subcommand)]
enum Command {
    /// Generates verification keys (and finalization hints) for all the basic & leaf circuits.
    /// Used for verification.
    #[command(name = "generate-vk")]
    GenerateVerificationKeys {},
    /// Generates setup keys (used by the CPU prover).
    #[command(name = "generate-sk")]
    GenerateSetupKeys {
        circuits_type: CircuitSelector,

        /// Specify for which circuit to generate the keys.
        #[arg(long)]
        numeric_circuit: Option<u8>,

        /// If true, then setup keys are not written and only md5 sum is printed.
        #[arg(long, default_value = "false")]
        dry_run: bool,
    },
    /// Generates setup keys (used by the GPU prover).
    #[command(name = "generate-sk-gpu")]
    GenerateGPUSetupKeys {
        circuits_type: CircuitSelector,

        /// Specify for which circuit to generate the keys.
        #[arg(long)]
        numeric_circuit: Option<u8>,

        /// If true, then setup keys are not written and only md5 sum is printed.
        #[arg(long, default_value = "false")]
        dry_run: bool,
    },
    /// Generates and updates the commitments - used by the verification contracts.
    #[command(name = "update-commitments")]
    UpdateCommitments {
        #[arg(long, default_value = "true")]
        dryrun: bool,
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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let opt = Cli::parse();
    // Setting key store from argument flags will come in next PR.
    let keystore = Keystore::default();

    match opt.command {
        Command::GenerateVerificationKeys {} => {
            tracing::info!(
                "Generating verification keys and storing them inside {:?}",
                keystore.get_base_path()
            );
            generate_vks(&keystore).context("generate_vks()")
        }
        Command::UpdateCommitments { dryrun } => read_and_update_contract_toml(&keystore, dryrun),

        Command::GenerateSetupKeys {
            circuits_type,
            numeric_circuit,
            dry_run,
        } => match circuits_type {
            CircuitSelector::All => {
                let digests = generate_all_cpu_setup_data(&keystore, dry_run)?;
                tracing::info!("CPU Setup keys md5(s):");
                print_stats(digests)
            }
            CircuitSelector::Recursive => {
                let digest = generate_cpu_setup_data(
                    &keystore,
                    false,
                    numeric_circuit.expect("--numeric-circuit must be provided"),
                    dry_run,
                )
                .context("generate_cpu_setup_data()")?;
                tracing::info!("digest: {:?}", digest);
                Ok(())
            }
            CircuitSelector::Basic => {
                let digest = generate_cpu_setup_data(
                    &keystore,
                    true,
                    numeric_circuit.expect("--numeric-circuit must be provided"),
                    dry_run,
                )
                .context("generate_cpu_setup_data()")?;
                tracing::info!("digest: {:?}", digest);
                Ok(())
            }
        },
        Command::GenerateGPUSetupKeys {
            circuits_type,
            numeric_circuit,
            dry_run,
        } => match circuits_type {
            CircuitSelector::All => {
                let digests = generate_all_gpu_setup_data(&keystore, dry_run)?;
                tracing::info!("GPU Setup keys md5(s):");
                print_stats(digests)
            }
            CircuitSelector::Recursive => {
                let digest = generate_gpu_setup_data(
                    &keystore,
                    false,
                    numeric_circuit.expect("--numeric-circuit must be provided"),
                    dry_run,
                )
                .context("generate_gpu_setup_data()")?;
                tracing::info!("digest: {:?}", digest);
                Ok(())
            }
            CircuitSelector::Basic => {
                let digest = generate_gpu_setup_data(
                    &keystore,
                    true,
                    numeric_circuit.expect("--numeric-circuit must be provided"),
                    dry_run,
                )
                .context("generate_gpu_setup_data()")?;
                tracing::info!("digest: {:?}", digest);
                Ok(())
            }
        },
    }
}
