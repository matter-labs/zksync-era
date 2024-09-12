use std::{sync::Arc, time::Instant};

use anyhow::Context;
use circuit_definitions::{
    circuit_definitions::recursion_layer::{
        recursion_tip::RecursionTipCircuit, ZkSyncRecursionLayerStorageType,
        ZkSyncRecursionLayerVerificationKey, ZkSyncRecursiveLayerCircuit, RECURSION_TIP_ARITY,
    },
    recursion_layer_proof_config,
};
use zkevm_test_harness::{
    boojum::{
        field::{
            goldilocks::{GoldilocksExt2, GoldilocksField},
            Field, U64Representable,
        },
        gadgets::{
            queue::QueueState, recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
            traits::allocatable::CSAllocatable,
        },
    },
    witness::{
        recursive_aggregation::compute_node_vk_commitment,
        utils::take_sponge_like_queue_state_from_simulator,
    },
    zkevm_circuits::{
        recursion::{
            leaf_layer::input::RecursionLeafParametersWitness,
            recursion_tip::{
                input::{RecursionTipInputWitness, RecursionTipInstanceWitness},
                RecursionTipConfig,
            },
        },
        scheduler::aux::BaseLayerCircuitType,
    },
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::keys::ClosedFormInputKey;
use zksync_prover_keystore::{keystore::Keystore, utils::get_leaf_vk_params};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};

use crate::{
    artifacts::ArtifactsManager, metrics::WITNESS_GENERATOR_METRICS, utils::ClosedFormInputWrapper,
};

mod artifacts;
mod job_processor;

#[derive(Clone)]
pub struct RecursionTipWitnessGeneratorJob {
    block_number: L1BatchNumber,
    recursion_tip_witness: RecursionTipInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    node_vk: ZkSyncRecursionLayerVerificationKey,
}

#[derive(Clone)]
pub struct RecursionTipArtifacts {
    pub recursion_tip_circuit: ZkSyncRecursiveLayerCircuit,
}

#[derive(Debug)]
pub struct RecursionTipWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
}

impl RecursionTipWitnessGenerator {
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
    ) -> Self {
        Self {
            config,
            object_store,
            prover_connection_pool,
            protocol_version,
            keystore,
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %job.block_number)
    )]
    pub fn process_job_sync(
        job: RecursionTipWitnessGeneratorJob,
        started_at: Instant,
    ) -> RecursionTipArtifacts {
        tracing::info!(
            "Starting fri witness generation of type {:?} for block {}",
            AggregationRound::RecursionTip,
            job.block_number.0
        );
        let config = RecursionTipConfig {
            proof_config: recursion_layer_proof_config(),
            vk_fixed_parameters: job.node_vk.clone().into_inner().fixed_parameters,
            _marker: std::marker::PhantomData,
        };

        let recursive_tip_circuit = RecursionTipCircuit {
            witness: job.recursion_tip_witness,
            config,
            transcript_params: (),
            _marker: std::marker::PhantomData,
        };

        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::RecursionTip.into()]
            .observe(started_at.elapsed());

        tracing::info!(
            "Recursion tip generation for block {} is complete in {:?}",
            job.block_number.0,
            started_at.elapsed()
        );

        RecursionTipArtifacts {
            recursion_tip_circuit: ZkSyncRecursiveLayerCircuit::RecursionTipCircuit(
                recursive_tip_circuit,
            ),
        }
    }
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %l1_batch_number)
)]
pub async fn prepare_job(
    l1_batch_number: L1BatchNumber,
    final_node_proof_job_ids: Vec<(u8, u32)>,
    object_store: &dyn ObjectStore,
    keystore: Keystore,
) -> anyhow::Result<RecursionTipWitnessGeneratorJob> {
    let started_at = Instant::now();
    let recursion_tip_proofs =
        RecursionTipWitnessGenerator::get_artifacts(&final_node_proof_job_ids, object_store)
            .await?;
    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::RecursionTip.into()]
        .observe(started_at.elapsed());

    let node_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
        )
        .context("get_recursive_layer_vk_for_circuit_type()")?;

    let node_layer_vk_commitment = compute_node_vk_commitment(node_vk.clone());

    let mut recursion_queues = vec![];
    for circuit_id in BaseLayerCircuitType::as_iter_u8() {
        let key = ClosedFormInputKey {
            block_number: l1_batch_number,
            circuit_id,
        };
        let ClosedFormInputWrapper(_, recursion_queue) = object_store.get(key).await?;
        recursion_queues.push((circuit_id, recursion_queue));
    }

    // RECURSION_TIP_ARITY is the maximum amount of proof that a single recursion tip can support.
    // Given recursion_tip has at most 1 proof per circuit, it implies we can't add more circuit types without bumping arity up.
    assert!(
        RECURSION_TIP_ARITY >= recursion_queues.len(),
        "recursion tip received more circuits ({}) than supported ({})",
        recursion_queues.len(),
        RECURSION_TIP_ARITY
    );
    let mut branch_circuit_type_set = [GoldilocksField::ZERO; RECURSION_TIP_ARITY];
    let mut queue_set: [_; RECURSION_TIP_ARITY] =
        std::array::from_fn(|_| QueueState::placeholder_witness());

    for (index, (circuit_id, recursion_queue)) in recursion_queues.iter().enumerate() {
        branch_circuit_type_set[index] = GoldilocksField::from_u64_unchecked(*circuit_id as u64);
        queue_set[index] = take_sponge_like_queue_state_from_simulator(recursion_queue);
    }

    let leaf_vk_commits = get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?;
    assert_eq!(
        leaf_vk_commits.len(),
        16,
        "expected 16 leaf vk commits, which corresponds to the numebr of circuits, got {}",
        leaf_vk_commits.len()
    );
    let leaf_layer_parameters: [RecursionLeafParametersWitness<GoldilocksField>; 16] =
        leaf_vk_commits
            .iter()
            .map(|el| el.1.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

    let input = RecursionTipInputWitness {
        leaf_layer_parameters,
        node_layer_vk_commitment,
        branch_circuit_type_set,
        queue_set,
    };

    let recursion_tip_witness = RecursionTipInstanceWitness {
        input,
        vk_witness: node_vk.clone().into_inner(),
        proof_witnesses: recursion_tip_proofs.into(),
    };

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::RecursionTip.into()]
        .observe(started_at.elapsed());

    Ok(RecursionTipWitnessGeneratorJob {
        block_number: l1_batch_number,
        recursion_tip_witness,
        node_vk,
    })
}
