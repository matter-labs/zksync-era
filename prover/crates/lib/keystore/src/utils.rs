use std::path::PathBuf;

use anyhow::Context as _;
use circuit_definitions::{
    circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit,
    snark_wrapper::franklin_crypto::bellman::{
        compact_bn256::Fq, pairing::bn256::Bn256,
        plonk::better_better_cs::setup::VerificationKey as SnarkVK,
    },
};
use sha3::Digest;
use zkevm_test_harness::{
    franklin_crypto::bellman::{CurveAffine, PrimeField, PrimeFieldRepr},
    witness::recursive_aggregation::compute_leaf_params,
};
use zksync_basic_types::H256;
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type,
    zkevm_circuits::{
        recursion::leaf_layer::input::RecursionLeafParametersWitness,
        scheduler::aux::BaseLayerCircuitType,
    },
};
use zksync_utils::locate_workspace;

use crate::keystore::Keystore;

pub fn get_leaf_vk_params(
    keystore: &Keystore,
) -> anyhow::Result<Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>> {
    let mut leaf_vk_commits = vec![];

    for circuit_type in BaseLayerCircuitType::as_iter_u8() {
        let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
            BaseLayerCircuitType::from_numeric_value(circuit_type),
        );
        let base_vk = keystore
            .load_base_layer_verification_key(circuit_type)
            .with_context(|| format!("get_base_layer_vk_for_circuit_type({circuit_type})"))?;
        let leaf_vk = keystore
            .load_recursive_layer_verification_key(recursive_circuit_type as u8)
            .with_context(|| {
                format!("get_recursive_layer_vk_for_circuit_type({recursive_circuit_type:?})")
            })?;
        let params = compute_leaf_params(circuit_type, base_vk, leaf_vk);
        leaf_vk_commits.push((circuit_type, params));
    }
    Ok(leaf_vk_commits)
}

/// Calculates the hash of a snark verification key.
// This function corresponds 1:1 with the following solidity code: https://github.com/matter-labs/era-contracts/blob/3e2bee96e412bac7c0a58c4b919837b59e9af36e/ethereum/contracts/zksync/Verifier.sol#L260
pub fn calculate_snark_vk_hash(keystore: &Keystore) -> anyhow::Result<H256> {
    let verification_key: SnarkVK<Bn256, ZkSyncSnarkWrapperCircuit> =
        serde_json::from_str(&keystore.load_snark_verification_key()?)?;

    let mut res = vec![];

    // gate setup commitments
    assert_eq!(8, verification_key.gate_setup_commitments.len());

    for gate_setup in verification_key.gate_setup_commitments {
        let (x, y) = gate_setup.as_xy();
        x.into_repr().write_be(&mut res).unwrap();
        y.into_repr().write_be(&mut res).unwrap();
    }

    // gate selectors commitments
    assert_eq!(2, verification_key.gate_selectors_commitments.len());

    for gate_selector in verification_key.gate_selectors_commitments {
        let (x, y) = gate_selector.as_xy();
        x.into_repr().write_be(&mut res).unwrap();
        y.into_repr().write_be(&mut res).unwrap();
    }

    // permutation commitments
    assert_eq!(4, verification_key.permutation_commitments.len());

    for permutation in verification_key.permutation_commitments {
        let (x, y) = permutation.as_xy();
        x.into_repr().write_be(&mut res).unwrap();
        y.into_repr().write_be(&mut res).unwrap();
    }

    // lookup selector commitment
    let lookup_selector = verification_key.lookup_selector_commitment.unwrap();
    let (x, y) = lookup_selector.as_xy();
    x.into_repr().write_be(&mut res).unwrap();
    y.into_repr().write_be(&mut res).unwrap();

    // lookup tables commitments
    assert_eq!(4, verification_key.lookup_tables_commitments.len());

    for table_commit in verification_key.lookup_tables_commitments {
        let (x, y) = table_commit.as_xy();
        x.into_repr().write_be(&mut res).unwrap();
        y.into_repr().write_be(&mut res).unwrap();
    }

    // table type commitment
    let lookup_table = verification_key.lookup_table_type_commitment.unwrap();
    let (x, y) = lookup_table.as_xy();
    x.into_repr().write_be(&mut res).unwrap();
    y.into_repr().write_be(&mut res).unwrap();

    // flag for using recursive part
    Fq::default().into_repr().write_be(&mut res).unwrap();

    let mut hasher = sha3::Keccak256::new();
    hasher.update(&res);
    let computed_vk_hash = hasher.finalize();

    Ok(H256::from_slice(&computed_vk_hash))
}

/// Returns workspace of the core component, we assume that prover is one folder deeper.
/// Or fallback to current dir
pub fn core_workspace_dir_or_current_dir() -> PathBuf {
    locate_workspace()
        .map(|a| a.join(".."))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use super::*;

    #[test]
    fn test_keyhash_generation() {
        let mut path_to_input = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        path_to_input.push("../../../data/historical_data");

        for entry in std::fs::read_dir(path_to_input.clone()).unwrap().flatten() {
            if entry.metadata().unwrap().is_dir() {
                let basepath = path_to_input.join(entry.file_name());
                let keystore = Keystore::new_with_optional_setup_path(basepath.clone(), None);

                let expected =
                    H256::from_str(&keystore.load_commitments().unwrap().snark_wrapper).unwrap();

                assert_eq!(
                    expected,
                    calculate_snark_vk_hash(&keystore).unwrap(),
                    "VK computation failed for {:?}",
                    basepath
                );
            }
        }
    }
}
