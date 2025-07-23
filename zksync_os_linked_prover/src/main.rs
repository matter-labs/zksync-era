use anyhow::Context;
use clap::Parser;
use proof_cache::client::ProofCacheClient;
use proof_cache::state::LinkedProof;
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use zksync_airbender_cli::prover_utils::{create_proofs_internal, create_recursion_proofs, generate_oracle_data_from_metadata_and_proof_list, load_binary_from_path, program_proof_from_proof_list_and_metadata, proof_list_and_metadata_from_program_proof, GpuSharedState, ProofMetadata, VerifierCircuitsIdentifiers};
use zksync_airbender_cli::Machine;
use zksync_airbender_execution_utils::{get_padded_binary, ProgramProof, UNIVERSAL_CIRCUIT_VERIFIER};

#[derive(Parser, Debug)]
#[command(name = "ZKsync OS Linked Prover")]
#[command(version = "0.0.1")]
#[command(about = "Linked FRI prover for ZKsync OS", long_about = None)]
struct Cli {
    /// Proof Cache URL to retrieve proofs from (i.e., "http://<IP>:<PORT>")
    #[arg(short, long, default_value = "http://localhost:3815", value_name = "PROOF_CACHE_URL")]
    proof_cache_url: String,

    // /// Number of the block to prove.
    // #[arg(short, long, required = true, value_name = "BLOCK_NUMBER")]
    // block_number: u32,
}

fn create_verification_proof(
    proof_metadata: &ProofMetadata,
    prover_input: Vec<u32>,
    binary: &Vec<u32>,
    gpu_state: &mut GpuSharedState,
) -> ProgramProof {
    let (current_proof_list, proof_metadata) = create_proofs_internal(
        &binary,
        prover_input,
        &Machine::Reduced,
        100, // Guessing - FIXME!!
        Some(proof_metadata.create_prev_metadata()),
        &mut Some(gpu_state),
        &mut Some(0f64),
    );
    program_proof_from_proof_list_and_metadata(&current_proof_list, &proof_metadata)
}

// async fn get_proofs(proof_cache_client: &ProofCacheClient) -> anyhow::Result<(LinkedProof, ProgramProof)> {
//     let linked_proof = proof_cache_client.get_linked().await.context("Failed to get linked proof")?;
//     tracing::info!("Loaded linked proof {linked_proof:?}");
//     let next_block_number = linked_proof.end_block_inclusive + 1;
//     let second_proof = proof_cache_client.get_fri(&(next_block_number).to_string()).await.context("Failed to get fri proof")?;
//     tracing::info!("Got FRI proof for block {}", next_block_number);
//     Ok((linked_proof, second_proof))
// }

fn get_proof_oracle(proof: &ProgramProof) -> Vec<u32> {
    let (metadata, list) = proof_list_and_metadata_from_program_proof(proof.clone());
    generate_oracle_data_from_metadata_and_proof_list(&metadata, &list)
}

fn merge_two(
    binary: &Vec<u32>,
    first_oracle: Vec<u32>,
    second_oracle: Vec<u32>,
    current_proof_metadata: &ProofMetadata,
    gpu_shared_state: &mut Option<&mut GpuSharedState>,
) -> ProgramProof {
    let mut merged = vec![VerifierCircuitsIdentifiers::CombinedRecursionLayers as u32];

    merged.extend(first_oracle);
    merged.extend(second_oracle);

    let mut timing = Some(0f64);
    println!("Previous metadata: {:#?}", current_proof_metadata.create_prev_metadata());
    let (current_proof_list, current_proof_metadata) = create_proofs_internal(
        &binary,
        merged,
        &Machine::Reduced,
        100, // Guessing - FIXME!!
        Some(current_proof_metadata.create_prev_metadata()),
        gpu_shared_state,
        &mut timing,
    );
    program_proof_from_proof_list_and_metadata(&current_proof_list, &current_proof_metadata)
}

/// TODO: Most likely to be absorbed in zksync_os_prover, but later.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let filter = EnvFilter::try_from_default_env()
    //     .unwrap_or_else(|_| {
    //         EnvFilter::new("info")
    //             .add_directive("gpu_prover=off".parse().expect("failed to parse gpu_prover hardcoded logging directive"))
    //             .add_directive("proof_cache=off".parse().expect("failed to parse proof_cache hardcoded logging directive"))
    //     });
    //
    // FmtSubscriber::builder()
    //     .with_env_filter(filter)
    //     .init();
    // // tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    //
    // let cli = Cli::parse();
    //
    // tracing::info!("ZKsync OS Linked Prover started connecting to proof cache at URL: {}", cli.proof_cache_url);
    //
    // let proof_cache_client = ProofCacheClient::new(cli.proof_cache_url)?;
    //
    // let verifier_binary = get_padded_binary(UNIVERSAL_CIRCUIT_VERIFIER);
    // let mut gpu_state = GpuSharedState::new(&verifier_binary);
    //
    // // let initial_proof = proof_cache_client.get_fri("1").await.context("Failed to get initial FRI proof")?;
    // // let (metadata, _) = proof_list_and_metadata_from_program_proof(initial_proof);
    //
    // loop {
    //     let mut proofs = Vec::new();
    //
    //     let mut block_number = 1;
    //
    //     loop {
    //         match proof_cache_client.get_fri(&block_number.to_string()).await {
    //             Ok(Some(proof)) => {
    //                 tracing::info!("Got FRI proof for block {}", block_number);
    //                 proofs.push(proof);
    //                 block_number += 1;
    //             }
    //             Ok(None) => {
    //                 tracing::info!("No more proofs available, stopping.");
    //                 break;
    //             }
    //             Err(e) => {
    //                 tracing::error!("Failed to get FRI proof for block {}: {}", block_number, e);
    //                 return Err(e.into());
    //             }
    //         }
    //     }
    //
    //     let (metadata, _) = proof_list_and_metadata_from_program_proof(proofs[0].clone());
    //
    //     let mut result = proofs[0].clone();
    //     for (id, next) in proofs.iter().skip(1).enumerate() {
    //         println!("Merging proof {} of {}", id + 1, proofs.len());
    //         let first_oracle = get_proof_oracle(&result);
    //         let next_oracle = get_proof_oracle(next);
    //
    //         // Merge each oracle with the first one.
    //         result = merge_two(
    //             &verifier_binary,
    //             first_oracle,
    //             next_oracle,
    //             &metadata,
    //             &mut Some(&mut gpu_state),
    //         );
    //     }
    //
    //     proof_cache_client.put_linked(LinkedProof { proof: result, start_block: 1, end_block_inclusive: block_number - 1 }).await?;
    // }
    // loop {
    //     let (linked_proof, next_proof) = get_proofs(&proof_cache_client).await.context("failed to get proofs")?;
    //
    //     let next_block_number = linked_proof.end_block_inclusive + 1;
    //
    //     tracing::info!("Starting to prove LinkedProof {linked_proof:?} with block {next_block_number}'s proof");
    //
    //     let first_oracle = get_proof_oracle(&linked_proof.proof);
    //     let second_oracle = get_proof_oracle(&next_proof);
    //
    //     let proof = merge_two(
    //         &verifier_binary,
    //         first_oracle,
    //         second_oracle,
    //         &metadata,
    //         &mut Some(&mut gpu_state),
    //     );
    //
    //     let linked_proof = LinkedProof {
    //         start_block: linked_proof.start_block,
    //         end_block_inclusive: next_block_number,
    //         proof,
    //     };
    //     tracing::info!("Finished proving verification for {linked_proof:?}");
    //
    //     proof_cache_client.put_linked(linked_proof).await.context("Failed to put linked proof")?;
    //
    //     tracing::info!("Finished submitting proof to cache");
    // }

    // =================================================

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            EnvFilter::new("info")
                .add_directive("gpu_prover=off".parse().expect("failed to parse gpu_prover hardcoded logging directive"))
                .add_directive("proof_cache=off".parse().expect("failed to parse proof_cache hardcoded logging directive"))
        });

    FmtSubscriber::builder()
        .with_env_filter(filter)
        .init();
    // tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    tracing::info!("ZKsync OS Linked Prover started connecting to proof cache at URL: {}", cli.proof_cache_url);

    let proof_cache_client = ProofCacheClient::new(cli.proof_cache_url)?;

    let verifier_binary = load_binary_from_path(&"/home/evl/code/zksync-airbender/tools/verifier/universal.bin".to_string());
    let mut gpu_state = GpuSharedState::new(&verifier_binary);

    // let proof = proof_cache_client.get_fri("1").await.context("Failed to get initial FRI proof")?;
    // let (metadata, _) = proof_list_and_metadata_from_program_proof(proof);


    loop {
        let linked_proof = proof_cache_client.get_linked().await.context("Failed to get linked proof")?;
        tracing::info!("Loaded linked proof {linked_proof:?}");
        let next_block_number = linked_proof.end_block_inclusive + 1;
        let second_proof = proof_cache_client.get_fri(&(next_block_number).to_string()).await.context("Failed to get fri proof")?;
        let second_proof = second_proof.unwrap();
        tracing::info!("Got FRI proof for block {}", next_block_number);

        tracing::info!("Starting to prove LinkedProof {linked_proof:?} with block {next_block_number}'s proof");

        let first_proof = linked_proof.proof;

        let (first_metadata, first_proof_list) = proof_list_and_metadata_from_program_proof(first_proof);
        let (second_metadata, second_proof_list) = proof_list_and_metadata_from_program_proof(second_proof);
        let first_oracle = generate_oracle_data_from_metadata_and_proof_list(&first_metadata, &first_proof_list);
        let second_oracle = generate_oracle_data_from_metadata_and_proof_list(&second_metadata, &second_proof_list);

        let mut merged_input = vec![VerifierCircuitsIdentifiers::CombinedRecursionLayers as u32];
        merged_input.extend(first_oracle);
        merged_input.extend(second_oracle);

        tracing::info!("Joined input for both proofs");

        tracing::info!("Proving verification...");
        let proof = create_verification_proof(&first_metadata, merged_input, &verifier_binary, &mut gpu_state);
        let linked_proof = LinkedProof {
            start_block: linked_proof.start_block,
            end_block_inclusive: next_block_number,
            proof,
        };
        tracing::info!("Finished proving verification for {linked_proof:?}");

        proof_cache_client.put_linked(linked_proof).await.context("Failed to put linked proof")?;

        tracing::info!("Finished submitting proof to cache");
    }
}

/*
 let (metadata, _) = proof_list_and_metadata_from_program_proof(proofs[0].clone());

    let mut gpu_shared_state = GpuSharedState::new(&get_padded_binary(UNIVERSAL_CIRCUIT_VERIFIER));
    //let mut gpu_shared_state

    let mut result = proofs.first().ok_or("No proofs provided")?.clone();
    for (id, next) in proofs.iter().skip(1).enumerate() {
        println!("Merging proof {} of {}", id + 1, proofs.len());
        let first_oracle = proof_to_recursion_oracle(&result);
        let next_oracle = proof_to_recursion_oracle(next);

        // Merge each oracle with the first one.
        result = merge_two(
            first_oracle,
            next_oracle,
            &metadata,
                 &mut Some(&mut gpu_shared_state),
            // &mut None,
        );
 */

/*
fn merge_two(
    first_oracle: Vec<u32>,
    second_oracle: Vec<u32>,
    current_proof_metadata: &ProofMetadata,
    gpu_shared_state: &mut Option<&mut GpuSharedState>,
) -> ProgramProof {
    let mut merged = vec![VerifierCircuitsIdentifiers::CombinedRecursionLayers as u32];

    merged.extend(first_oracle);
    merged.extend(second_oracle);

    //u32_to_file(&"merged".to_string(), &merged);

    let binary = get_padded_binary(UNIVERSAL_CIRCUIT_VERIFIER);
    let mut timing = Some(0f64);
    let (current_proof_list, current_proof_metadata) = create_proofs_internal(
        &binary,
        merged,
        &Machine::Reduced,
        100, // Guessing - FIXME!!
        Some(current_proof_metadata.create_prev_metadata()),
        gpu_shared_state,
        &mut timing,
    );
    program_proof_from_proof_list_and_metadata(&current_proof_list, &current_proof_metadata)
}
 */