use crate::get_recursive_layer_vk_for_circuit_type;
use crate::utils::get_leaf_vk_params;
use anyhow::Context as _;
use once_cell::sync::Lazy;
use std::str::FromStr;
use structopt::lazy_static::lazy_static;
use zkevm_test_harness::witness::recursive_aggregation::{
    compute_leaf_vks_and_params_commitment, compute_node_vk_commitment,
};
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_types::protocol_version::{L1VerifierConfig, VerifierParams};
use zksync_types::H256;

lazy_static! {
    // TODO: do not initialize a static const with data read in runtime.
    static ref COMMITMENTS: Lazy<L1VerifierConfig> = Lazy::new(|| { circuit_commitments().unwrap() });
}

pub struct VkCommitments {
    pub leaf: String,
    pub node: String,
    pub scheduler: String,
}

fn circuit_commitments() -> anyhow::Result<L1VerifierConfig> {
    let commitments = generate_commitments().context("generate_commitments()")?;
    let snark_wrapper_vk = std::env::var("CONTRACTS_SNARK_WRAPPER_VK_HASH")
        .context("SNARK wrapper VK not found in the config")?;
    Ok(L1VerifierConfig {
        params: VerifierParams {
            recursion_node_level_vk_hash: H256::from_str(&commitments.node)
                .context("invalid node commitment")?,
            recursion_leaf_level_vk_hash: H256::from_str(&commitments.leaf)
                .context("invalid leaf commitment")?,
            // The base layer commitment is not used in the FRI prover verification.
            recursion_circuits_set_vks_hash: H256::zero(),
        },
        // Instead of loading the FRI scheduler verification key here,
        // we load the SNARK-wrapper verification key.
        // This is due to the fact that these keys are used only for picking the
        // prover jobs / witgen jobs from the DB. The keys are matched with the ones in
        // `prover_protocol_versions` table, which has the SNARK-wrapper verification key.
        // This is OK because if the FRI VK changes, the SNARK-wrapper VK will change as well.
        // You can actually compute the SNARK-wrapper VK from the FRI VK, but this is not yet
        // implemented in the `zkevm_test_harness`, so instead we're loading it from the env.
        recursion_scheduler_level_vk_hash: H256::from_str(&snark_wrapper_vk)
            .context("invalid SNARK wrapper VK")?,
    })
}

pub fn generate_commitments() -> anyhow::Result<VkCommitments> {
    let leaf_vk_params = get_leaf_vk_params().context("get_leaf_vk_params()")?;
    let leaf_layer_params = leaf_vk_params
        .iter()
        .map(|el| el.1.clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let leaf_vk_commitment = compute_leaf_vks_and_params_commitment(leaf_layer_params);

    let node_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
    )
    .context("get_recursive_layer_vk_for_circuit_type(NodeLayerCircuit)")?;
    let node_vk_commitment = compute_node_vk_commitment(node_vk.clone());

    let scheduler_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
    )
    .context("get_recursive_layer_vk_for_circuit_type(SchedulerCircuit)")?;
    let scheduler_vk_commitment = compute_node_vk_commitment(scheduler_vk.clone());

    let hex_concatenator = |hex_array: [GoldilocksField; 4]| {
        "0x".to_owned()
            + &hex_array
                .iter()
                .map(|x| format!("{:016x}", x.0))
                .collect::<Vec<_>>()
                .join("")
    };

    let leaf_aggregation_commitment_hex = hex_concatenator(leaf_vk_commitment);
    let node_aggregation_commitment_hex = hex_concatenator(node_vk_commitment);
    let scheduler_commitment_hex = hex_concatenator(scheduler_vk_commitment);
    tracing::info!(
        "leaf aggregation commitment {:?}",
        leaf_aggregation_commitment_hex
    );
    tracing::info!(
        "node aggregation commitment {:?}",
        node_aggregation_commitment_hex
    );
    tracing::info!("scheduler commitment {:?}", scheduler_commitment_hex);
    Ok(VkCommitments {
        leaf: leaf_aggregation_commitment_hex,
        node: node_aggregation_commitment_hex,
        scheduler: scheduler_commitment_hex,
    })
}

pub fn get_cached_commitments() -> L1VerifierConfig {
    tracing::info!("Using cached commitments {:?}", **COMMITMENTS);
    **COMMITMENTS
}

#[test]
fn test_get_cached_commitments() {
    let commitments = get_cached_commitments();
    assert_eq!(
        H256::zero(),
        commitments.params.recursion_circuits_set_vks_hash
    )
}
