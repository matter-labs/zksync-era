use std::{str::FromStr, sync::Mutex};

use anyhow::Context as _;
use hex::ToHex;
use once_cell::sync::Lazy;
use zkevm_test_harness::witness::recursive_aggregation::{
    compute_leaf_vks_and_params_commitment, compute_node_vk_commitment,
};
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
};
use zksync_types::{protocol_version::L1VerifierConfig, H256};

use crate::{
    keystore::Keystore,
    utils::{calculate_snark_vk_hash, get_leaf_vk_params},
    VkCommitments,
};

static KEYSTORE: Lazy<Mutex<Option<Keystore>>> = Lazy::new(|| Mutex::new(None));

fn circuit_commitments(keystore: &Keystore) -> anyhow::Result<L1VerifierConfig> {
    let commitments = generate_commitments(keystore).context("generate_commitments()")?;
    Ok(L1VerifierConfig {
        // Instead of loading the FRI scheduler verification key here,
        // we load the SNARK-wrapper verification key.
        // This is due to the fact that these keys are used only for picking the
        // prover jobs / witgen jobs from the DB. The keys are matched with the ones in
        // `prover_fri_protocol_versions` table, which has the SNARK-wrapper verification key.
        // This is OK because if the FRI VK changes, the SNARK-wrapper VK will change as well.
        recursion_scheduler_level_vk_hash: H256::from_str(&commitments.snark_wrapper)
            .context("invalid SNARK wrapper VK")?,
    })
}

pub fn generate_commitments(keystore: &Keystore) -> anyhow::Result<VkCommitments> {
    let leaf_vk_params = get_leaf_vk_params(keystore).context("get_leaf_vk_params()")?;
    let leaf_layer_params = leaf_vk_params
        .iter()
        .map(|el| el.1.clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let leaf_vk_commitment = compute_leaf_vks_and_params_commitment(leaf_layer_params);

    let node_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
        )
        .context("get_recursive_layer_vk_for_circuit_type(NodeLayerCircuit)")?;
    let node_vk_commitment = compute_node_vk_commitment(node_vk.clone());

    let scheduler_vk = keystore
        .load_recursive_layer_verification_key(
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
    let snark_vk_hash: String = calculate_snark_vk_hash(keystore)?.encode_hex();

    let result = VkCommitments {
        leaf: leaf_aggregation_commitment_hex,
        node: node_aggregation_commitment_hex,
        scheduler: scheduler_commitment_hex,
        snark_wrapper: format!("0x{}", snark_vk_hash),
    };
    tracing::info!("Commitments: {:?}", result);
    Ok(result)
}

pub fn get_cached_commitments(setup_data_path: Option<String>) -> L1VerifierConfig {
    if let Some(setup_data_path) = setup_data_path {
        let keystore = Keystore::new_with_setup_data_path(setup_data_path);
        let mut keystore_lock = KEYSTORE.lock().unwrap();
        *keystore_lock = Some(keystore);
    }

    let keystore = KEYSTORE.lock().unwrap().clone().unwrap_or_default();
    let commitments = circuit_commitments(&keystore).unwrap();

    tracing::info!("Using cached commitments {:?}", commitments);
    commitments
}
