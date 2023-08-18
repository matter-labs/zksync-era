#![feature(generic_const_exprs)]
#![feature(allocator_api)]

use std::fs;
use std::fs::File;
use std::io::Read;
use zksync_prover_fri_types::circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;
use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::round_function::AbsorptionModeOverwrite;
use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::sponge::GenericAlgebraicSponge;

use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::hints::{
    DenseVariablesCopyHint, DenseWitnessCopyHint,
};
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::polynomial_storage::{
    SetupBaseStorage, SetupStorage,
};
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::verifier::VerificationKey;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::oracle::merkle_tree::MerkleTreeWithCap;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::oracle::TreeHasher;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::boojum::field::{PrimeField, SmallField};

use zksync_prover_fri_types::circuit_definitions::boojum::field::traits::field_like::PrimeFieldLikeVectorized;
use zksync_prover_fri_types::circuit_definitions::boojum::implementations::poseidon2::Poseidon2Goldilocks;
use zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker;

use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::{
    ZkSyncBaseLayerCircuit, ZkSyncBaseLayerVerificationKey,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey,
};
use zksync_prover_fri_types::circuit_definitions::{
    ZkSyncDefaultRoundFunction, BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR,
};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zkevm_test_harness::prover_utils::create_base_layer_setup_data;
use zksync_config::configs::FriProverConfig;
use zksync_types::proofs::AggregationRound;

pub mod in_memory_setup_data_source;
pub mod utils;
use zksync_prover_fri_types::ProverServiceDataKey;
#[cfg(feature = "gpu")]
use {shivini::cs::GpuSetup, std::alloc::Global};

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    bound = "F: serde::Serialize + serde::de::DeserializeOwned, P: serde::Serialize + serde::de::DeserializeOwned"
)]
pub struct ProverSetupData<
    F: PrimeField + SmallField,
    P: PrimeFieldLikeVectorized<Base = F>,
    H: TreeHasher<F>,
> {
    pub setup_base: SetupBaseStorage<F, P>,
    pub setup: SetupStorage<F, P>,
    #[serde(bound(
        serialize = "H::Output: serde::Serialize",
        deserialize = "H::Output: serde::de::DeserializeOwned"
    ))]
    pub vk: VerificationKey<F, H>,
    #[serde(bound(
        serialize = "H::Output: serde::Serialize",
        deserialize = "H::Output: serde::de::DeserializeOwned"
    ))]
    pub setup_tree: MerkleTreeWithCap<F, H>,
    pub vars_hint: DenseVariablesCopyHint,
    pub wits_hint: DenseWitnessCopyHint,
    pub finalization_hint: FinalizationHintsForProver,
}

pub type GoldilocksProverSetupData = ProverSetupData<
    GoldilocksField,
    GoldilocksField,
    GenericAlgebraicSponge<
        GoldilocksField,
        GoldilocksField,
        8,
        12,
        4,
        Poseidon2Goldilocks,
        AbsorptionModeOverwrite,
    >,
>;

#[cfg(feature = "gpu")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound = "F: serde::Serialize + serde::de::DeserializeOwned")]
pub struct GpuProverSetupData<F: PrimeField + SmallField, H: TreeHasher<F>> {
    pub setup: GpuSetup<Global>,
    #[serde(bound(
        serialize = "H::Output: serde::Serialize",
        deserialize = "H::Output: serde::de::DeserializeOwned"
    ))]
    pub vk: VerificationKey<F, H>,
    pub finalization_hint: FinalizationHintsForProver,
}

#[cfg(feature = "gpu")]
pub type GoldilocksGpuProverSetupData = GpuProverSetupData<
    GoldilocksField,
    GenericAlgebraicSponge<
        GoldilocksField,
        GoldilocksField,
        8,
        12,
        4,
        Poseidon2Goldilocks,
        AbsorptionModeOverwrite,
    >,
>;

pub enum ProverServiceDataType {
    VerificationKey,
    SetupData,
    FinalizationHints,
}

pub fn get_base_path() -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!(
        "{}/prover/vk_setup_data_generator_server_fri/data",
        zksync_home
    )
}

pub fn get_file_path(
    key: ProverServiceDataKey,
    service_data_type: ProverServiceDataType,
) -> String {
    let name = match key.round {
        AggregationRound::BasicCircuits => {
            format!("basic_{}", key.circuit_id)
        }
        AggregationRound::LeafAggregation => {
            format!("leaf_{}", key.circuit_id)
        }
        AggregationRound::NodeAggregation => "node".to_string(),
        AggregationRound::Scheduler => "scheduler".to_string(),
    };
    match service_data_type {
        ProverServiceDataType::VerificationKey => {
            format!("{}/verification_{}_key.json", get_base_path(), name)
        }
        ProverServiceDataType::SetupData => {
            format!(
                "{}/setup_{}_data.bin",
                FriProverConfig::from_env().setup_data_path,
                name
            )
        }
        ProverServiceDataType::FinalizationHints => {
            format!("{}/finalization_hints_{}.bin", get_base_path(), name)
        }
    }
}

pub fn get_base_layer_vk_for_circuit_type(circuit_type: u8) -> ZkSyncBaseLayerVerificationKey {
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .unwrap_or_else(|_| panic!("Failed reading verification key from path: {}", filepath));
    serde_json::from_str::<ZkSyncBaseLayerVerificationKey>(&text).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing verification key from path: {}",
            filepath
        )
    })
}

pub fn get_recursive_layer_vk_for_circuit_type(
    circuit_type: u8,
) -> ZkSyncRecursionLayerVerificationKey {
    let round = get_round_for_recursive_circuit_type(circuit_type);
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, round),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .unwrap_or_else(|_| panic!("Failed reading verification key from path: {}", filepath));
    serde_json::from_str::<ZkSyncRecursionLayerVerificationKey>(&text).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing verification key from path: {}",
            filepath
        )
    })
}

pub fn get_round_for_recursive_circuit_type(circuit_type: u8) -> AggregationRound {
    match circuit_type {
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8 => {
            AggregationRound::Scheduler
        }
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8 => {
            AggregationRound::NodeAggregation
        }
        _ => AggregationRound::LeafAggregation,
    }
}

pub fn save_base_layer_vk(vk: ZkSyncBaseLayerVerificationKey) {
    let circuit_type = vk.numeric_circuit_type();
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("saving basic verification key to: {}", filepath);
    std::fs::write(filepath, serde_json::to_string_pretty(&vk).unwrap()).unwrap();
}

pub fn save_recursive_layer_vk(vk: ZkSyncRecursionLayerVerificationKey) {
    let circuit_type = vk.numeric_circuit_type();
    let round = get_round_for_recursive_circuit_type(circuit_type);
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, round),
        ProverServiceDataType::VerificationKey,
    );
    vlog::info!("saving recursive layer verification key to: {}", filepath);
    std::fs::write(filepath, serde_json::to_string_pretty(&vk).unwrap()).unwrap();
}

pub fn get_cpu_setup_data_for_circuit_type<F, P, H>(
    key: ProverServiceDataKey,
) -> ProverSetupData<F, P, H>
where
    F: PrimeField + SmallField + Serialize + DeserializeOwned,
    P: PrimeFieldLikeVectorized<Base = F> + Serialize + DeserializeOwned,
    H: TreeHasher<F>,
    <H as TreeHasher<F>>::Output: Serialize + DeserializeOwned,
{
    let filepath = get_file_path(key.clone(), ProverServiceDataType::SetupData);
    let mut file = File::open(filepath.clone())
        .unwrap_or_else(|_| panic!("Failed reading setup-data from path: {:?}", filepath));
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap_or_else(|_| {
        panic!(
            "Failed reading setup-data to buffer from path: {:?}",
            filepath
        )
    });
    vlog::info!("loading {:?} setup data from path: {}", key, filepath);
    bincode::deserialize::<ProverSetupData<F, P, H>>(&buffer).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing setup-data at path: {:?} for circuit: {:?}",
            filepath, key
        )
    })
}

#[cfg(feature = "gpu")]
pub fn get_setup_data_for_circuit_type<F, H>(key: ProverServiceDataKey) -> GpuProverSetupData<F, H>
where
    F: PrimeField + SmallField + Serialize + DeserializeOwned,
    H: TreeHasher<F>,
    <H as TreeHasher<F>>::Output: Serialize + DeserializeOwned,
{
    let filepath = get_file_path(key.clone(), ProverServiceDataType::SetupData);
    let mut file = File::open(filepath.clone())
        .unwrap_or_else(|_| panic!("Failed reading setup-data from path: {:?}", filepath));
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap_or_else(|_| {
        panic!(
            "Failed reading setup-data to buffer from path: {:?}",
            filepath
        )
    });
    vlog::info!("loading {:?} setup data from path: {}", key, filepath);
    bincode::deserialize::<GpuProverSetupData<F, H>>(&buffer).unwrap_or_else(|_| {
        panic!(
            "Failed deserializing setup-data at path: {:?} for circuit: {:?}",
            filepath, key
        )
    })
}

pub fn save_setup_data(key: ProverServiceDataKey, serialized_setup_data: &Vec<u8>) {
    let filepath = get_file_path(key.clone(), ProverServiceDataType::SetupData);
    vlog::info!("saving {:?} setup data to: {}", key, filepath);
    std::fs::write(filepath.clone(), serialized_setup_data)
        .unwrap_or_else(|_| panic!("Failed saving setup-data at path: {:?}", filepath));
}

pub fn generate_cpu_base_layer_setup_data(
    circuit: ZkSyncBaseLayerCircuit<
        GoldilocksField,
        VmWitnessOracle<GoldilocksField>,
        ZkSyncDefaultRoundFunction,
    >,
) -> GoldilocksProverSetupData {
    let circuit_type = circuit.numeric_circuit_type();
    vlog::info!(
        "starting setup data generator for base layer circuit: {}.",
        circuit_type
    );
    let worker = Worker::new();
    let (setup_base, setup, vk, setup_tree, vars_hint, wits_hint, finalization_hint) =
        create_base_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
    let key = ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits);
    let existing_finalization_hint = get_finalization_hints(key);
    assert_eq!(
        existing_finalization_hint, finalization_hint,
        "finalization hint mismatch for circuit: {circuit_type}"
    );
    let existing_vk = get_base_layer_vk_for_circuit_type(circuit_type);
    assert_eq!(
        existing_vk.into_inner(),
        vk,
        "vk mismatch for circuit: {circuit_type}"
    );
    ProverSetupData {
        setup_base,
        setup,
        vk,
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    }
}

pub fn save_finalization_hints(key: ProverServiceDataKey, hint: &FinalizationHintsForProver) {
    let filepath = get_file_path(key.clone(), ProverServiceDataType::FinalizationHints);
    vlog::info!("saving finalization hints for {:?} to: {}", key, filepath);
    let serialized = bincode::serialize(&hint).expect("Failed to serialize finalization hints");
    fs::write(filepath, serialized).expect("Failed to write finalization hints to file");
}
pub fn get_finalization_hints(key: ProverServiceDataKey) -> FinalizationHintsForProver {
    let filepath = get_file_path(key, ProverServiceDataType::FinalizationHints);
    let file = fs::read(filepath).expect("Failed to read finalization hints from file");
    bincode::deserialize::<FinalizationHintsForProver>(&file)
        .expect("Finalization hint deserialization failed")
}
