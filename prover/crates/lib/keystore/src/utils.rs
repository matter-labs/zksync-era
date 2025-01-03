use anyhow::Context as _;
use circuit_definitions::{
    circuit_definitions::aux_layer::{
        ZkSyncSnarkWrapperCircuit, ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
    },
    snark_wrapper::franklin_crypto::bellman::{
        compact_bn256::Fq, pairing::bn256::Bn256,
        plonk::better_better_cs::setup::VerificationKey as SnarkVK,
    },
};
use fflonk::FflonkVerificationKey;
use sha3::Digest;
use zkevm_test_harness::{
    franklin_crypto::bellman::{CurveAffine, PrimeField, PrimeFieldRepr},
    witness::recursive_aggregation::compute_leaf_params,
};
use zksync_basic_types::{H256, U256};
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type,
    zkevm_circuits::{
        recursion::leaf_layer::input::RecursionLeafParametersWitness,
        scheduler::aux::BaseLayerCircuitType,
    },
};

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
pub fn calculate_snark_vk_hash(verification_key: String) -> anyhow::Result<H256> {
    let verification_key: SnarkVK<Bn256, ZkSyncSnarkWrapperCircuit> =
        serde_json::from_str(&verification_key)?;

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

pub fn calculate_fflonk_snark_vk_hash(verification_key: String) -> anyhow::Result<H256> {
    let verification_key: FflonkVerificationKey<
        Bn256,
        ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
    > = serde_json::from_str(&verification_key)?;

    let mut res = vec![0u8; 32];

    // NUM INPUTS
    // Writing as u256 to comply with the contract
    let num_inputs = U256::from(verification_key.num_inputs);
    num_inputs.to_big_endian(&mut res[0..32]);

    // C0 G1
    let c0_g1 = verification_key.c0;
    let (x, y) = c0_g1.as_xy();

    x.into_repr().write_be(&mut res)?;
    y.into_repr().write_be(&mut res)?;

    // NON RESIDUES
    let non_residues = verification_key.non_residues;
    for non_residue in non_residues {
        non_residue.into_repr().write_be(&mut res)?;
    }

    // G2 ELEMENTS
    let g2_elements = verification_key.g2_elements;
    for g2_element in g2_elements {
        res.extend(g2_element.into_uncompressed().as_ref());
    }

    let mut hasher = sha3::Keccak256::new();
    hasher.update(&res);
    let computed_vk_hash = hasher.finalize();

    Ok(H256::from_slice(&computed_vk_hash))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde::{Deserialize, Serialize};
    use zksync_utils::env::Workspace;

    use super::*;

    // Helper type, since the interface of VK commitments is changed with FFLONK
    #[derive(Serialize, Deserialize)]
    pub struct VkCommitmentsLegacy {
        pub leaf: String,
        pub node: String,
        pub scheduler: String,
        // Hash computed over Snark verification key fields.
        pub snark_wrapper: String,
    }

    #[test]
    fn test_keyhash_generation() {
        let path_to_input = Workspace::locate().prover().join("data/historical_data");

        for entry in std::fs::read_dir(path_to_input.clone()).unwrap().flatten() {
            if entry.metadata().unwrap().is_dir() {
                let basepath = path_to_input.join(entry.file_name());
                let filepath = basepath.join("commitments.json");

                let text = std::fs::read_to_string(&filepath)
                    .unwrap_or_else(|_| panic!("File at {:?} should be read", filepath));

                let commitments = serde_json::from_str::<VkCommitmentsLegacy>(&text)
                    .expect("Vk commitments should be deserialized correctly");

                let expected = H256::from_str(&commitments.snark_wrapper).unwrap();

                let key = if std::fs::exists(basepath.join("verification_snark_key.json")).unwrap()
                {
                    std::fs::read_to_string(basepath.join("verification_snark_key.json")).unwrap()
                } else {
                    std::fs::read_to_string(basepath.join("snark_verification_scheduler_key.json"))
                        .unwrap()
                };

                let calculated = calculate_snark_vk_hash(key).unwrap();

                assert_eq!(
                    expected, calculated,
                    "VK computation failed for {:?}",
                    basepath
                );
            }
        }
    }
}
