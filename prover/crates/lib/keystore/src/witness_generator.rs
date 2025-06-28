use anyhow::Context;
use circuit_definitions::circuit_definitions::{
    base_layer::ZkSyncBaseLayerVerificationKey,
    recursion_layer::ZkSyncRecursionLayerVerificationKey,
};
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
};
use zksync_types::H256;
use zksync_witness_generator_service::rounds::VerificationKeyManager;

use crate::{keystore::Keystore, utils::get_leaf_vk_params};

impl VerificationKeyManager for Keystore {
    fn load_base_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncBaseLayerVerificationKey> {
        self.load_base_layer_verification_key(circuit_type)
            .context("Failed to load base layer verification key")
    }

    fn load_recursive_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncRecursionLayerVerificationKey> {
        self.load_recursive_layer_verification_key(circuit_type)
            .context("Failed to load recursive layer verification key")
    }

    fn verify_scheduler_vk_hash(&self, expected_hash: H256) -> anyhow::Result<()> {
        self.verify_scheduler_vk_hash(expected_hash)
            .context("Failed to verify scheduler verification key hash")
    }

    fn get_leaf_vk_params(
        &self,
    ) -> anyhow::Result<Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>> {
        get_leaf_vk_params(&self).context("Failed to get leaf verification key parameters")
    }
}
