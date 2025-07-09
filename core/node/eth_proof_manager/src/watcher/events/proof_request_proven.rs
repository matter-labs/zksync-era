use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use bellman::{
    bn256::Fr, plonk::commitments::transcript::keccak_transcript::RollingKeccakTranscript,
};
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuitNoLookupCustomGate;
use ethabi::{decode, ParamType, Token};
use zksync_dal::{eth_watcher_dal::EventType, ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, StoredObject};
use zksync_prover_interface::outputs::{
    L1BatchProofForL1, L1BatchProofForL1Key, TypedL1BatchProofForL1,
};
use zksync_types::{api::Log, ethabi, h256_to_u256, L1BatchNumber, H256, U256};

use crate::{
    types::{FflonkFinalVerificationKey, PlonkFinalVerificationKey, ProvingNetwork},
    watcher::events::EventHandler,
};

// event ProofRequestProven(
//    uint256 indexed chainId, uint256 indexed blockNumber, bytes proof, ProvingNetwork assignedTo
//);
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProofRequestProven {
    pub chain_id: U256,
    pub block_number: U256,
    pub proof: Vec<u8>,
    pub assigned_to: ProvingNetwork,
}

pub struct ProofRequestProvenHandler {
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    fflonk_vk: FflonkFinalVerificationKey,
    plonk_vk: PlonkFinalVerificationKey,
}

impl ProofRequestProvenHandler {
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        fflonk_vk: FflonkFinalVerificationKey,
        plonk_vk: PlonkFinalVerificationKey,
    ) -> Self {
        Self {
            connection_pool,
            blob_store,
            fflonk_vk,
            plonk_vk,
        }
    }
}

#[async_trait]
impl EventHandler for ProofRequestProvenHandler {
    fn signature(&self) -> H256 {
        ethabi::long_signature(
            "ProofRequestProven",
            &[
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Bytes,
                // ProvingNetwork is enum, encoded as uint8
                ethabi::ParamType::Uint(8),
            ],
        )
    }

    fn event_type(&self) -> EventType {
        EventType::ProofRequestProven
    }

    async fn handle(&self, log: Log) -> anyhow::Result<()> {
        if log.topics.len() != 3 {
            return Err(anyhow::anyhow!(
                "invalid number of topics: {:?}, expected 3",
                log.topics
            ));
        }

        if *log.topics.get(0).context("missing topic 0")? != self.signature() {
            return Err(anyhow::anyhow!(
                "invalid signature: {:?}, expected {:?}",
                log.topics.get(0),
                self.signature()
            ));
        }

        let chain_id = h256_to_u256(*log.topics.get(1).context("missing topic 1")?);
        let block_number = h256_to_u256(*log.topics.get(2).context("missing topic 2")?);

        let decoded = decode(&[ParamType::Bytes, ParamType::Uint(8)], &log.data.0)?;

        let proof = match &decoded[0] {
            Token::Bytes(b) => b.clone(),
            _ => panic!("Expected bytes"),
        };

        let assigned_to = match &decoded[1] {
            Token::Uint(u) => ProvingNetwork::from_u256(*u),
            _ => panic!("Expected uint8"),
        };

        let event = ProofRequestProven {
            chain_id,
            block_number,
            proof: proof.clone(),
            assigned_to,
        };

        tracing::info!(
            "Received ProofRequestProvenEvent for batch {}, chain_id: {}, assigned_to: {:?}",
            event.block_number,
            event.chain_id,
            event.assigned_to,
        );

        let proof = <L1BatchProofForL1 as StoredObject>::deserialize(proof)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize proof: {}", e))?;

        let verification_result = match proof.inner() {
            TypedL1BatchProofForL1::Fflonk(proof) => {
                let proof = proof.scheduler_proof;
                fflonk::verify::<
                    _,
                    ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
                    RollingKeccakTranscript<Fr>,
                >(&self.fflonk_vk, &proof, None)
                .map_err(|e| anyhow::anyhow!("Failed to verify fflonk proof: {}", e))?
            }
            TypedL1BatchProofForL1::Plonk(proof) => {
                let proof = proof.scheduler_proof;

                bellman::plonk::better_better_cs::verifier::verify::<
                    _,
                    _,
                    RollingKeccakTranscript<Fr>,
                >(&self.plonk_vk, &proof, None)
                .map_err(|e| anyhow::anyhow!("Failed to verify plonk proof: {}", e))?
            }
        };

        let batch_number = L1BatchNumber(block_number.as_u32());

        if verification_result {
            let proof_blob_url = self
                .blob_store
                .put(
                    L1BatchProofForL1Key::Core((batch_number, proof.protocol_version())),
                    &proof,
                )
                .await?;

            self.connection_pool
                .connection()
                .await?
                .proof_generation_dal()
                .save_proof_artifacts_metadata(batch_number, &proof_blob_url)
                .await?;
        }

        tracing::info!(
            "Batch {}, chain_id: {}, assigned_to: {:?}, verification_result: {:?}",
            batch_number,
            event.chain_id,
            event.assigned_to,
            verification_result
        );

        self.connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .mark_batch_as_proven(batch_number, verification_result)
            .await?;

        Ok(())
    }
}
