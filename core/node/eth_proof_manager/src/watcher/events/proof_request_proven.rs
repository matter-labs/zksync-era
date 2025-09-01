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
use zksync_types::{
    api::Log, commitment::serialize_commitments, ethabi, h256_to_u256, web3::keccak256,
    L1BatchNumber, ProtocolVersionId, H256, STATE_DIFF_HASH_KEY_PRE_GATEWAY, U256,
};

use crate::{
    types::{FflonkFinalVerificationKey, ProvingNetwork},
    watcher::events::EventHandler,
};

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
}

impl ProofRequestProvenHandler {
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        fflonk_vk: FflonkFinalVerificationKey,
    ) -> Self {
        Self {
            connection_pool,
            blob_store,
            fflonk_vk,
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

        if *log.topics.first().context("missing topic 0")? != self.signature() {
            return Err(anyhow::anyhow!(
                "invalid signature: {:?}, expected {:?}",
                log.topics.first(),
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

        let batch_number = L1BatchNumber(event.block_number.as_u32());

        let verification_result = match verify_proof(
            self.connection_pool.clone(),
            batch_number,
            proof.clone(),
            self.fflonk_vk.clone(),
        )
        .await
        {
            Ok(proof) => {
                tracing::info!("Proof for batch {} verified successfully", batch_number);

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
                true
            }
            Err(e) => {
                tracing::error!("Failed to verify proof for batch {}: {}", batch_number, e);
                false
            }
        };

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

async fn verify_proof(
    connection_pool: ConnectionPool<Core>,
    batch_number: L1BatchNumber,
    proof_bytes: Vec<u8>,
    verification_key: FflonkFinalVerificationKey,
) -> anyhow::Result<L1BatchProofForL1> {
    let proof = <L1BatchProofForL1 as StoredObject>::deserialize(proof)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize proof: {}", e))?;

    let verification_result = match proof.inner() {
        TypedL1BatchProofForL1::Fflonk(proof) => {
            let proof = proof.scheduler_proof;
            fflonk::verify::<
                _,
                ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
                RollingKeccakTranscript<Fr>,
            >(&verification_key, &proof, None)
            .map_err(|e| anyhow::anyhow!("Failed to verify fflonk proof: {}", e))?
        }
        TypedL1BatchProofForL1::Plonk(_) => {
            return Err(anyhow::anyhow!(
                "Plonk proofs are not supported by proving networks"
            ));
        }
    };

    if !verification_result {
        return Err(anyhow::anyhow!("Verify function returned false"));
    }

    let aggregation_coords = proof.aggregation_result_coords();

    let system_logs_hash_from_prover = H256::from_slice(&aggregation_coords[0]);
    let state_diff_hash_from_prover = H256::from_slice(&aggregation_coords[1]);
    let bootloader_heap_initial_content_from_prover = H256::from_slice(&aggregation_coords[2]);
    let events_queue_state_from_prover = H256::from_slice(&aggregation_coords[3]);

    let mut storage = connection_pool.connection().await?;

    let l1_batch = storage
        .blocks_dal()
        .get_l1_batch_metadata(batch_number)
        .await?
        .ok_or(anyhow::anyhow!("Proved block without metadata"))?;

    let protocol_version = l1_batch
        .header
        .protocol_version
        .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

    let events_queue_state = l1_batch
        .metadata
        .events_queue_commitment
        .ok_or(anyhow::anyhow!("No events_queue_commitment"))?;
    let bootloader_heap_initial_content =
        l1_batch
            .metadata
            .bootloader_initial_content_commitment
            .ok_or(anyhow::anyhow!("No bootloader_initial_content_commitment"))?;

    if events_queue_state != events_queue_state_from_prover
        || bootloader_heap_initial_content != bootloader_heap_initial_content_from_prover
    {
        return Err(anyhow::anyhow!(
                    "Auxilary output doesn't match\n\
                    server values: events_queue_state = {events_queue_state}, bootloader_heap_initial_content = {bootloader_heap_initial_content}\n\
                    prover values: events_queue_state = {events_queue_state_from_prover}, bootloader_heap_initial_content = {bootloader_heap_initial_content_from_prover}",
                ));
    }

    let system_logs = serialize_commitments(&l1_batch.header.system_logs);
    let system_logs_hash = H256(keccak256(&system_logs));

    let state_diff_hash = if protocol_version.is_pre_gateway() {
        l1_batch
            .header
            .system_logs
            .iter()
            .find_map(|log| {
                (log.0.key == H256::from_low_u64_be(STATE_DIFF_HASH_KEY_PRE_GATEWAY as u64))
                    .then_some(log.0.value)
            })
            .ok_or(anyhow::anyhow!(
                "Failed to get state_diff_hash from system logs"
            ))?
    } else {
        l1_batch.metadata.state_diff_hash.ok_or(anyhow::anyhow!(
            "Failed to get state_diff_hash from metadata"
        ))?
    };

    if state_diff_hash != state_diff_hash_from_prover
        || system_logs_hash != system_logs_hash_from_prover
    {
        let server_values =
            format!("system_logs_hash = {system_logs_hash}, state_diff_hash = {state_diff_hash}");
        let prover_values = format!("system_logs_hash = {system_logs_hash_from_prover}, state_diff_hash = {state_diff_hash_from_prover}");
        return Err(anyhow::anyhow!(
            "Auxilary output doesn't match, server values: {} prover values: {}",
            server_values,
            prover_values
        ));
    }

    Ok(proof)
}
