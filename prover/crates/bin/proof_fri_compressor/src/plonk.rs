use anyhow::Context as _;
use circuit_sequencer_api::proof::FinalProof;
use wrapper_prover::{GPUWrapperConfigs, WrapperProver};
use zkevm_test_harness::proof_wrapper_utils::{get_trusted_setup, DEFAULT_WRAPPER_CONFIG};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
};
use zksync_prover_keystore::keystore::Keystore;

use crate::compressor::Proof;

#[tracing::instrument(skip(proof))]
pub(crate) async fn generate_plonk_proof(
    proof: ZkSyncRecursionLayerProof,
    keystore: Keystore,
) -> anyhow::Result<Proof> {
    let scheduler_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        )
        .context("get_recursiver_layer_vk_for_circuit_type()")?;

    let wrapper_proof = {
        let crs = get_trusted_setup();
        let wrapper_config = DEFAULT_WRAPPER_CONFIG;
        let mut prover = WrapperProver::<GPUWrapperConfigs>::new(&crs, wrapper_config).unwrap();

        prover
            .generate_setup_data(scheduler_vk.into_inner())
            .unwrap();
        prover.generate_proofs(proof.into_inner()).unwrap();

        prover.get_wrapper_proof().unwrap()
    };

    // (Re)serialization should always succeed.
    let serialized = bincode::serialize(&wrapper_proof)
        .expect("Failed to serialize proof with ZkSyncSnarkWrapperCircuit");

    // For sending to L1, we can use the `FinalProof` type, that has a generic circuit inside, that is not used for serialization.
    // So `FinalProof` and `Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>` are compatible on serialization bytecode level.
    let final_proof: FinalProof =
        bincode::deserialize(&serialized).expect("Failed to deserialize final proof");
    Ok(Proof::Plonk(Box::new(final_proof)))
}
