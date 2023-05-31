use prover_service::ArtifactProvider;
use std::io::Read;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncVerificationKey;
use zkevm_test_harness::pairing::bn256::Bn256;
use zksync_setup_key_server::get_setup_for_circuit_type;
use zksync_verification_key_server::get_vk_for_circuit_type;

#[derive(Debug)]
pub struct ProverArtifactProvider;

impl ArtifactProvider for ProverArtifactProvider {
    type ArtifactError = String;

    fn get_setup(&self, circuit_id: u8) -> Result<Box<dyn Read>, Self::ArtifactError> {
        Ok(get_setup_for_circuit_type(circuit_id))
    }

    fn get_vk(&self, circuit_id: u8) -> Result<ZkSyncVerificationKey<Bn256>, Self::ArtifactError> {
        let vk = get_vk_for_circuit_type(circuit_id);
        Ok(ZkSyncVerificationKey::from_verification_key_and_numeric_type(circuit_id, vk))
    }
}
