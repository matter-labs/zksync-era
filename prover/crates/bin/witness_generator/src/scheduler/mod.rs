use std::{convert::TryInto, sync::Arc, time::Instant};

use anyhow::Context as _;
use zkevm_test_harness::zkevm_circuits::recursion::{
    leaf_layer::input::RecursionLeafParametersWitness, NUM_BASE_LAYER_CIRCUITS,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        circuit_definitions::recursion_layer::{
            scheduler::SchedulerCircuit, ZkSyncRecursionLayerStorageType,
            ZkSyncRecursionLayerVerificationKey, ZkSyncRecursiveLayerCircuit, SCHEDULER_CAPACITY,
        },
        recursion_layer_proof_config,
        zkevm_circuits::scheduler::{input::SchedulerCircuitInstanceWitness, SchedulerConfig},
    },
    FriProofWrapper,
};
use zksync_prover_keystore::{keystore::Keystore, utils::get_leaf_vk_params};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};

use crate::{
    artifacts::ArtifactsManager, metrics::WITNESS_GENERATOR_METRICS,
    utils::SchedulerPartialInputWrapper,
};

mod artifacts;
mod job_processor;

#[derive(Clone)]
pub struct SchedulerArtifacts {
    pub scheduler_circuit: ZkSyncRecursiveLayerCircuit,
}

#[derive(Clone)]
pub struct SchedulerWitnessGeneratorJob {
    block_number: L1BatchNumber,
    scheduler_witness: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    node_vk: ZkSyncRecursionLayerVerificationKey,
    recursion_tip_vk: ZkSyncRecursionLayerVerificationKey,
    leaf_layer_parameters:
        [RecursionLeafParametersWitness<GoldilocksField>; NUM_BASE_LAYER_CIRCUITS],
}

#[derive(Debug)]
pub struct SchedulerWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
}

impl SchedulerWitnessGenerator {
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
        job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> SchedulerArtifacts {
        tracing::info!(
            "Starting fri witness generation of type {:?} for block {}",
            AggregationRound::Scheduler,
            job.block_number.0
        );
        let config = SchedulerConfig {
            proof_config: recursion_layer_proof_config(),
            vk_fixed_parameters: job.recursion_tip_vk.clone().into_inner().fixed_parameters,
            capacity: SCHEDULER_CAPACITY,
            _marker: std::marker::PhantomData,
            recursion_tip_vk: job.recursion_tip_vk.into_inner(),
            node_layer_vk: job.node_vk.into_inner(),
            leaf_layer_parameters: job.leaf_layer_parameters,
        };

        let scheduler_circuit = SchedulerCircuit {
            witness: job.scheduler_witness,
            config,
            transcript_params: (),
            _marker: std::marker::PhantomData,
        };
        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::Scheduler.into()]
            .observe(started_at.elapsed());

        tracing::info!(
            "Scheduler generation for block {} is complete in {:?}",
            job.block_number.0,
            started_at.elapsed()
        );

        SchedulerArtifacts {
            scheduler_circuit: ZkSyncRecursiveLayerCircuit::SchedulerCircuit(scheduler_circuit),
        }
    }
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %l1_batch_number)
)]
pub async fn prepare_job(
    l1_batch_number: L1BatchNumber,
    recursion_tip_job_id: u32,
    object_store: &dyn ObjectStore,
    keystore: Keystore,
) -> anyhow::Result<SchedulerWitnessGeneratorJob> {
    let started_at = Instant::now();
    let wrapper =
        SchedulerWitnessGenerator::get_artifacts(&recursion_tip_job_id, object_store).await?;
    let recursion_tip_proof = match wrapper {
        FriProofWrapper::Base(_) => Err(anyhow::anyhow!(
            "Expected only recursive proofs for scheduler l1 batch {l1_batch_number}, got Base"
        )),
        FriProofWrapper::Recursive(recursive_proof) => Ok(recursive_proof.into_inner()),
    }?;
    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::Scheduler.into()]
        .observe(started_at.elapsed());

    let started_at = Instant::now();
    let node_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
        )
        .context("get_recursive_layer_vk_for_circuit_type()")?;
    let SchedulerPartialInputWrapper(mut scheduler_witness) =
        object_store.get(l1_batch_number).await?;

    let recursion_tip_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8,
        )
        .context("get_recursion_tip_vk()")?;
    scheduler_witness.proof_witnesses = vec![recursion_tip_proof].into();

    let leaf_vk_commits = get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?;
    let leaf_layer_parameters = leaf_vk_commits
        .iter()
        .map(|el| el.1.clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::Scheduler.into()]
        .observe(started_at.elapsed());

    Ok(SchedulerWitnessGeneratorJob {
        block_number: l1_batch_number,
        scheduler_witness,
        node_vk,
        leaf_layer_parameters,
        recursion_tip_vk,
    })
}
