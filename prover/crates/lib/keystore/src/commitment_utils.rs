use std::str::FromStr;

use anyhow::Context as _;
use hex::ToHex;
use zkevm_test_harness::witness::recursive_aggregation::{
    compute_leaf_vks_and_params_commitment, compute_node_vk_commitment,
};
use zksync_basic_types::H256;
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
};

use crate::{
    keystore::Keystore,
    utils::{calculate_fflonk_snark_vk_hash, calculate_snark_vk_hash, get_leaf_vk_params},
    VkCommitments,
};

impl Keystore {
    pub fn generate_commitments(&self) -> anyhow::Result<VkCommitments> {
        let leaf_vk_params = get_leaf_vk_params(self).context("get_leaf_vk_params()")?;
        let leaf_layer_params = leaf_vk_params
            .iter()
            .map(|el| el.1.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let leaf_vk_commitment = compute_leaf_vks_and_params_commitment(leaf_layer_params);

        let node_vk = self
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            )
            .context("get_recursive_layer_vk_for_circuit_type(NodeLayerCircuit)")?;
        let node_vk_commitment = compute_node_vk_commitment(node_vk.clone());

        let scheduler_vk = self
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
        let plonk_snark_vk_hash: String =
            calculate_snark_vk_hash(self.load_snark_verification_key().unwrap())?.encode_hex();
        let fflonk_snark_vk_hash: String =
            calculate_fflonk_snark_vk_hash(self.load_fflonk_snark_verification_key().unwrap())?
                .encode_hex();

        let result = VkCommitments {
            leaf: leaf_aggregation_commitment_hex,
            node: node_aggregation_commitment_hex,
            scheduler: scheduler_commitment_hex,
            snark_wrapper: format!("0x{}", plonk_snark_vk_hash),
            fflonk_snark_wrapper: format!("0x{}", fflonk_snark_vk_hash),
        };
        tracing::info!("Commitments: {:?}", result);
        Ok(result)
    }

    pub fn verify_scheduler_vk_hash(&self, expected_hash: H256) -> anyhow::Result<()> {
        let commitments = self
            .generate_commitments()
            .context("generate_commitments()")?;
        let calculated_hash =
            H256::from_str(&commitments.snark_wrapper).context("invalid SNARK wrapper VK")?;
        anyhow::ensure!(expected_hash == calculated_hash, "Invalid SNARK wrapper VK hash. Calculated locally: {calculated_hash:?}, provided: {expected_hash:?}");
        Ok(())
    }
}
