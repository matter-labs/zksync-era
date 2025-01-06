use std::time::Instant;

use anyhow::Context;
use fflonk_gpu::{fflonk::FflonkProof, FflonkSnarkVerifierCircuitDeviceSetup};
use proof_compression_gpu::{FflonkSnarkVerifierCircuit, FflonkSnarkVerifierCircuitProof};
use tokio::sync::mpsc::{Receiver, Sender};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::{
    aux_layer::{
        wrapper::ZkSyncCompressionWrapper, ZkSyncCompressionForWrapperCircuit,
        ZkSyncCompressionLayerCircuit, ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    },
    recursion_layer::{
        ZkSyncRecursionLayerProof, ZkSyncRecursionLayerVerificationKey,
        ZkSyncRecursionVerificationKey,
    },
};
use zksync_prover_keystore::keystore::Keystore;

pub(crate) struct SnarkSetupDataLoader {
    pub keystore: Keystore,
    pub sender: Sender<FflonkSnarkVerifierCircuitDeviceSetup>,
}

impl SnarkSetupDataLoader {
    pub fn new(keystore: Keystore, sender: Sender<FflonkSnarkVerifierCircuitDeviceSetup>) -> Self {
        Self { keystore, sender }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let started_at = Instant::now();

        tracing::info!("Started loading SNARK setup data");

        let setup_data = self.keystore.load_fflonk_snark_verifier_setup_data()?;

        tracing::info!(
            "Finished loading SNARK setup data, time taken: {:?}",
            started_at.elapsed()
        );

        let started_at = Instant::now();

        match self.sender.send(setup_data).await {
            Ok(_) => {
                tracing::info!(
                    "Finished transferring SNARK setup data, time taken: {:?}",
                    started_at.elapsed()
                );
                Ok(())
            }
            Err(err) => {
                tracing::error!("Failed to transfer SNARK setup data with error: {:?}", err);
                Err(err.into())
            }
        }
    }
}

pub(crate) struct CrsLoader {
    pub path: String,
    pub sender: Sender<()>,
}

impl CrsLoader {
    pub fn new(path: String, sender: Sender<()>) -> Self {
        Self { path, sender }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        tracing::info!("Started loading CRS file");

        let started_at = Instant::now();

        // todo: implement loading CRS

        tracing::info!(
            "Finished loading CRS file, time taken: {:?}",
            started_at.elapsed()
        );

        let started_at = Instant::now();

        match self.sender.send(()).await {
            Ok(_) => {
                tracing::info!(
                    "Finished transferring CRS data, time taken: {:?}",
                    started_at.elapsed()
                );
                Ok(())
            }
            Err(err) => {
                tracing::error!("Failed to transfer CRS data with error: {:?}", err);
                Err(err.into())
            }
        }
    }
}

pub(crate) struct Compressor {
    pub keystore: Keystore,
    pub compression_mode: u8,
    pub proof: ZkSyncRecursionLayerProof,
    pub vk: ZkSyncRecursionVerificationKey,
    pub sender: Sender<(
        ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    )>,
}

impl Compressor {
    pub fn new(
        keystore: Keystore,
        compression_mode: u8,
        proof: ZkSyncRecursionLayerProof,
        vk: ZkSyncRecursionLayerVerificationKey,
        sender: Sender<(
            ZkSyncCompressionProofForWrapper,
            ZkSyncCompressionVerificationKeyForWrapper,
        )>,
    ) -> Self {
        Self {
            keystore,
            compression_mode,
            proof,
            vk: vk.into_inner(),
            sender,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let worker = franklin_crypto::boojum::worker::Worker::new();
        let mut compression_circuit = ZkSyncCompressionLayerCircuit::from_witness_and_vk(
            Some(self.proof.clone().into_inner()),
            self.vk.clone(),
            1,
        );
        let mut compression_wrapper_circuit = None;

        let started_at = Instant::now();

        tracing::info!("Started proof compression");

        for step_idx in 1..self.compression_mode {
            tracing::info!("Proving compression {:?}", step_idx);
            let setup_data = self.keystore.load_compression_setup_data(step_idx)?;
            let (proof, vk) =
                proof_compression_gpu::prove_compression_layer_circuit_with_precomputations(
                    compression_circuit.clone(),
                    &setup_data.setup,
                    setup_data.finalization_hint,
                    setup_data.vk,
                    &worker,
                );
            tracing::info!("Proof for compression {:?} is generated!", step_idx);

            if step_idx + 1 == self.compression_mode {
                compression_wrapper_circuit =
                    Some(ZkSyncCompressionForWrapperCircuit::from_witness_and_vk(
                        Some(proof),
                        vk,
                        self.compression_mode,
                    ));
            } else {
                compression_circuit = ZkSyncCompressionLayerCircuit::from_witness_and_vk(
                    Some(proof),
                    vk,
                    step_idx + 1,
                );
            }
        }

        // last wrapping step
        tracing::info!("Proving compression {} for wrapper", self.compression_mode);

        let setup_data = self
            .keystore
            .load_compression_wrapper_setup_data(self.compression_mode)?;
        let (proof, vk) =
            proof_compression_gpu::prove_compression_wrapper_circuit_with_precomputations(
                compression_wrapper_circuit.unwrap(),
                &setup_data.setup,
                setup_data.finalization_hint,
                setup_data.vk,
                &worker,
            );
        tracing::info!(
            "Proof for compression wrapper {} is generated!",
            self.compression_mode
        );

        tracing::info!("Finished generating proofs for compression chain with compression mode {}, time taken: {:?}", self.compression_mode, started_at.elapsed());

        let started_at = Instant::now();
        match self.sender.send((proof, vk)).await {
            Ok(_) => {
                tracing::info!(
                    "Finished transferring compressed proof, time taken: {:?}",
                    started_at.elapsed()
                );
                Ok(())
            }
            Err(err) => {
                tracing::error!("Failed to transfer compressed proof with error: {:?}", err);
                Err(err.into())
            }
        }
    }
}

pub(crate) struct SnarkProver {
    pub setup_data_receiver: Receiver<FflonkSnarkVerifierCircuitDeviceSetup>,
    pub crs_receiver: Receiver<()>,
    pub compression_receiver: Receiver<(
        ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    )>,
    pub compression_mode: u8,
}

impl SnarkProver {
    pub fn new(
        setup_data_receiver: Receiver<FflonkSnarkVerifierCircuitDeviceSetup>,
        crs_receiver: Receiver<()>,
        compression_receiver: Receiver<(
            ZkSyncCompressionProofForWrapper,
            ZkSyncCompressionVerificationKeyForWrapper,
        )>,
        compression_mode: u8,
    ) -> Self {
        Self {
            setup_data_receiver,
            crs_receiver,
            compression_receiver,
            compression_mode,
        }
    }

    pub async fn run(mut self) -> anyhow::Result<FflonkSnarkVerifierCircuitProof> {
        let started_at = Instant::now();

        tracing::info!("Started SNARK prover, waiting for data...");

        let setup = self
            .setup_data_receiver
            .recv()
            .await
            .context("Failed to load setup data")?;
        let _crs = self
            .crs_receiver
            .recv()
            .await
            .context("Failed to load CRS file")?;
        let (compression_wrapper_proof, compression_wrapper_vk) = self
            .compression_receiver
            .recv()
            .await
            .context("Failed to receive compression proof and verification key")?;

        tracing::info!(
            "Received data for SNARK proving, waiting time: {:?}",
            started_at.elapsed()
        );

        let started_at = Instant::now();

        let fixed_parameters = compression_wrapper_vk.fixed_parameters.clone();

        // construct fflonk snark verifier circuit
        let wrapper_function =
            ZkSyncCompressionWrapper::from_numeric_circuit_type(self.compression_mode);

        let circuit = FflonkSnarkVerifierCircuit {
            witness: Some(compression_wrapper_proof),
            vk: compression_wrapper_vk,
            fixed_parameters,
            transcript_params: (),
            wrapper_function,
        };

        let proof = fflonk_gpu::gpu_prove_fflonk_snark_verifier_circuit_with_precomputation(
            &circuit,
            &setup,
            &setup.get_verification_key(),
        );

        tracing::info!(
            "Finished proof generation, time taken: {:?}",
            started_at.elapsed()
        );

        Ok(proof)
    }
}
