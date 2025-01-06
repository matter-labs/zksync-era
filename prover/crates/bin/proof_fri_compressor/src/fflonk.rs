use anyhow::Context as _;
use fflonk_gpu::FflonkSnarkVerifierCircuitDeviceSetup;
use proof_compression_gpu::{FflonkSnarkVerifierCircuit, FflonkSnarkVerifierCircuitProof};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::{
    aux_layer::{
        wrapper::ZkSyncCompressionWrapper, ZkSyncCompressionForWrapperCircuit,
        ZkSyncCompressionLayerCircuit, ZkSyncCompressionProof, ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    },
    recursion_layer::{
        ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType, ZkSyncRecursionVerificationKey,
    },
};
use zksync_prover_keystore::keystore::Keystore;

use crate::compressor::Proof;

#[tracing::instrument(skip(proof, compression_mode, keystore))]
pub(crate) async fn generate_fflonk_proof(
    proof: ZkSyncRecursionLayerProof,
    compression_mode: u8,
    keystore: Keystore,
) -> anyhow::Result<Proof> {
    let scheduler_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        )
        .context("get_recursiver_layer_vk_for_circuit_type()")?;

    let (compression_result, setup) = tokio::join!(
        tokio::spawn(compress_proof(
            keystore.clone(),
            proof.into_inner(),
            scheduler_vk.into_inner(),
            compression_mode
        )),
        tokio::spawn(load_setup(keystore.clone()))
    );

    // construct fflonk snark verifier circuit
    let wrapper_function = ZkSyncCompressionWrapper::from_numeric_circuit_type(compression_mode);

    let (compression_wrapper_proof, compression_wrapper_vk) = compression_result??;
    let fixed_parameters = compression_wrapper_vk.fixed_parameters.clone();
    let circuit = FflonkSnarkVerifierCircuit {
        witness: Some(compression_wrapper_proof),
        vk: compression_wrapper_vk,
        fixed_parameters,
        transcript_params: (),
        wrapper_function,
    };

    let setup = setup?;

    let proof = fflonk_gpu::gpu_prove_fflonk_snark_verifier_circuit_with_precomputation(
        &circuit,
        &setup,
        &setup.get_verification_key(),
    );

    tracing::info!("Finished proof generation");
    Ok(Proof::Fflonk(proof))
}

async fn load_setup(keystore: Keystore) -> FflonkSnarkVerifierCircuitDeviceSetup {
    keystore.load_fflonk_snark_verifier_setup_data().unwrap()
}

async fn compress_proof(
    keystore: Keystore,
    proof: ZkSyncCompressionProof,
    vk: ZkSyncRecursionVerificationKey,
    compression_steps: u8,
) -> anyhow::Result<(
    ZkSyncCompressionProofForWrapper,
    ZkSyncCompressionVerificationKeyForWrapper,
)> {
    let worker = franklin_crypto::boojum::worker::Worker::new();
    let mut compression_circuit =
        ZkSyncCompressionLayerCircuit::from_witness_and_vk(Some(proof), vk.clone(), 1);
    let mut compression_wrapper_circuit = None;

    for step_idx in 1..compression_steps {
        tracing::info!("Proving compression {:?}", step_idx);
        let setup_data = keystore.load_compression_setup_data(step_idx)?;
        let (proof, vk) =
            proof_compression_gpu::prove_compression_layer_circuit_with_precomputations(
                compression_circuit.clone(),
                &setup_data.setup,
                setup_data.finalization_hint,
                setup_data.vk,
                &worker,
            );
        tracing::info!("Proof for compression {:?} is generated!", step_idx);

        if step_idx + 1 == compression_steps {
            compression_wrapper_circuit =
                Some(ZkSyncCompressionForWrapperCircuit::from_witness_and_vk(
                    Some(proof),
                    vk,
                    compression_steps,
                ));
        } else {
            compression_circuit =
                ZkSyncCompressionLayerCircuit::from_witness_and_vk(Some(proof), vk, step_idx + 1);
        }
    }

    // last wrapping step
    tracing::info!("Proving compression {} for wrapper", compression_steps);

    let setup_data = keystore.load_compression_wrapper_setup_data(compression_steps)?;
    let (proof, vk) = proof_compression_gpu::prove_compression_wrapper_circuit_with_precomputations(
        compression_wrapper_circuit.unwrap(),
        &setup_data.setup,
        setup_data.finalization_hint,
        setup_data.vk,
        &worker,
    );
    tracing::info!(
        "Proof for compression wrapper {} is generated!",
        compression_steps
    );
    Ok((proof, vk))
}
