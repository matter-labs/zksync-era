#![feature(generic_const_exprs)]
#![feature(allocator_api)]

use std::{fs, fs::File, io::Read};

use anyhow::Context as _;
use circuit_definitions::circuit_definitions::aux_layer::{
    ZkSyncCompressionLayerStorageType, ZkSyncSnarkWrapperVK,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use zkevm_test_harness::prover_utils::create_base_layer_setup_data;
use zksync_config::configs::FriProverConfig;
use zksync_env_config::FromEnv;
use zksync_prover_fri_types::{
    circuit_definitions::{
        aux_definitions::witness_oracle::VmWitnessOracle,
        boojum::{
            algebraic_props::{
                round_function::AbsorptionModeOverwrite, sponge::GenericAlgebraicSponge,
            },
            cs::{
                implementations::{
                    hints::{DenseVariablesCopyHint, DenseWitnessCopyHint},
                    polynomial_storage::{SetupBaseStorage, SetupStorage},
                    setup::FinalizationHintsForProver,
                    verifier::VerificationKey,
                },
                oracle::{merkle_tree::MerkleTreeWithCap, TreeHasher},
            },
            field::{
                goldilocks::GoldilocksField, traits::field_like::PrimeFieldLikeVectorized,
                PrimeField, SmallField,
            },
            implementations::poseidon2::Poseidon2Goldilocks,
            worker::Worker,
        },
        circuit_definitions::{
            base_layer::{ZkSyncBaseLayerCircuit, ZkSyncBaseLayerVerificationKey},
            recursion_layer::{
                ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey,
            },
        },
        ZkSyncDefaultRoundFunction, BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR,
    },
    ProverServiceDataKey,
};
use zksync_types::{
    proofs::AggregationRound,
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::{
            bn256::Bn256, plonk::better_better_cs::setup::VerificationKey as SnarkVerificationKey,
        },
        witness::oracle::VmWitnessOracle as SnarkWitnessOracle,
    },
};
#[cfg(feature = "gpu")]
use {shivini::cs::GpuSetup, std::alloc::Global};

pub mod commitment_utils;
pub mod utils;
pub mod vk_commitment_helper;

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
    SnarkVerificationKey,
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
) -> anyhow::Result<String> {
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
    Ok(match service_data_type {
        ProverServiceDataType::VerificationKey => {
            format!("{}/verification_{}_key.json", get_base_path(), name)
        }
        ProverServiceDataType::SetupData => {
            format!(
                "{}/setup_{}_data.bin",
                FriProverConfig::from_env()
                    .context("FriProverConfig::from_env()")?
                    .setup_data_path,
                name
            )
        }
        ProverServiceDataType::FinalizationHints => {
            format!("{}/finalization_hints_{}.bin", get_base_path(), name)
        }
        ProverServiceDataType::SnarkVerificationKey => {
            format!("{}/snark_verification_{}_key.json", get_base_path(), name)
        }
    })
}

pub fn get_base_layer_vk_for_circuit_type(
    circuit_type: u8,
) -> anyhow::Result<ZkSyncBaseLayerVerificationKey> {
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        ProverServiceDataType::VerificationKey,
    )
    .context("get_file_path()")?;
    tracing::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .with_context(|| format!("Failed reading verification key from path: {filepath}"))?;
    serde_json::from_str::<ZkSyncBaseLayerVerificationKey>(&text)
        .with_context(|| format!("Failed deserializing verification key from path: {filepath}"))
}

pub fn get_recursive_layer_vk_for_circuit_type(
    circuit_type: u8,
) -> anyhow::Result<ZkSyncRecursionLayerVerificationKey> {
    let round = get_round_for_recursive_circuit_type(circuit_type);
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, round),
        ProverServiceDataType::VerificationKey,
    )
    .context("get_file_path()")?;
    tracing::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .with_context(|| format!("Failed reading verification key from path: {filepath}"))?;
    serde_json::from_str::<ZkSyncRecursionLayerVerificationKey>(&text)
        .with_context(|| format!("Failed deserializing verification key from path: {filepath}"))
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

pub fn save_base_layer_vk(vk: ZkSyncBaseLayerVerificationKey) -> anyhow::Result<()> {
    let circuit_type = vk.numeric_circuit_type();
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        ProverServiceDataType::VerificationKey,
    )
    .context("get_file_path()")?;
    tracing::info!("saving basic verification key to: {}", filepath);
    std::fs::write(&filepath, serde_json::to_string_pretty(&vk).unwrap())
        .with_context(|| format!("writing to '{filepath}' failed"))
}

pub fn save_recursive_layer_vk(vk: ZkSyncRecursionLayerVerificationKey) -> anyhow::Result<()> {
    let circuit_type = vk.numeric_circuit_type();
    let round = get_round_for_recursive_circuit_type(circuit_type);
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_type, round),
        ProverServiceDataType::VerificationKey,
    )
    .context("get_file_path()")?;
    tracing::info!("saving recursive layer verification key to: {}", filepath);
    std::fs::write(&filepath, serde_json::to_string_pretty(&vk).unwrap())
        .with_context(|| format!("writing to '{filepath}' failed"))
}

pub fn save_snark_vk(vk: ZkSyncSnarkWrapperVK) -> anyhow::Result<()> {
    let filepath = get_file_path(
        ProverServiceDataKey::new(vk.numeric_circuit_type(), AggregationRound::Scheduler),
        ProverServiceDataType::SnarkVerificationKey,
    )
    .context("get_file_path()")?;
    tracing::info!("saving snark verification key to: {}", filepath);
    fs::write(
        &filepath,
        serde_json::to_string_pretty(&vk.into_inner()).unwrap(),
    )
    .with_context(|| format!("writing to '{filepath}' failed"))
}

pub fn get_cpu_setup_data_for_circuit_type<F, P, H>(
    key: ProverServiceDataKey,
) -> anyhow::Result<ProverSetupData<F, P, H>>
where
    F: PrimeField + SmallField + Serialize + DeserializeOwned,
    P: PrimeFieldLikeVectorized<Base = F> + Serialize + DeserializeOwned,
    H: TreeHasher<F>,
    <H as TreeHasher<F>>::Output: Serialize + DeserializeOwned,
{
    let filepath =
        get_file_path(key.clone(), ProverServiceDataType::SetupData).context("get_file_path()")?;
    let mut file = File::open(filepath.clone())
        .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("Failed reading setup-data to buffer from path: {filepath:?}"))?;
    tracing::info!("loading {:?} setup data from path: {}", key, filepath);
    bincode::deserialize::<ProverSetupData<F, P, H>>(&buffer).with_context(|| {
        format!("Failed deserializing setup-data at path: {filepath:?} for circuit: {key:?}")
    })
}

#[cfg(feature = "gpu")]
pub fn get_setup_data_for_circuit_type<F, H>(
    key: ProverServiceDataKey,
) -> anyhow::Result<GpuProverSetupData<F, H>>
where
    F: PrimeField + SmallField + Serialize + DeserializeOwned,
    H: TreeHasher<F>,
    <H as TreeHasher<F>>::Output: Serialize + DeserializeOwned,
{
    let filepath =
        get_file_path(key.clone(), ProverServiceDataType::SetupData).context("get_file_path()")?;
    let mut file = File::open(filepath.clone())
        .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("Failed reading setup-data to buffer from path: {filepath:?}"))?;
    tracing::info!("loading {:?} setup data from path: {}", key, filepath);
    bincode::deserialize::<GpuProverSetupData<F, H>>(&buffer).with_context(|| {
        format!("Failed deserializing setup-data at path: {filepath:?} for circuit: {key:?}")
    })
}

pub fn save_setup_data(
    key: ProverServiceDataKey,
    serialized_setup_data: &Vec<u8>,
) -> anyhow::Result<()> {
    let filepath =
        get_file_path(key.clone(), ProverServiceDataType::SetupData).context("get_file_path()")?;
    tracing::info!("saving {:?} setup data to: {}", key, filepath);
    std::fs::write(filepath.clone(), serialized_setup_data)
        .with_context(|| format!("Failed saving setup-data at path: {filepath:?}"))
}

pub fn generate_cpu_base_layer_setup_data(
    circuit: ZkSyncBaseLayerCircuit<
        GoldilocksField,
        VmWitnessOracle<GoldilocksField>,
        ZkSyncDefaultRoundFunction,
    >,
) -> anyhow::Result<GoldilocksProverSetupData> {
    let circuit_type = circuit.numeric_circuit_type();
    tracing::info!(
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
    let existing_finalization_hint =
        get_finalization_hints(key).context("get_finalization_hints()")?;
    if existing_finalization_hint != finalization_hint {
        anyhow::bail!("finalization hint mismatch for circuit: {circuit_type}");
    }
    let existing_vk = get_base_layer_vk_for_circuit_type(circuit_type)
        .with_context(|| format!("get_base_layer_vk_for_circuit_type({circuit_type})"))?;
    if existing_vk.into_inner() != vk {
        anyhow::bail!("vk mismatch for circuit: {circuit_type}");
    }
    Ok(ProverSetupData {
        setup_base,
        setup,
        vk,
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    })
}

pub fn save_finalization_hints(
    key: ProverServiceDataKey,
    hint: &FinalizationHintsForProver,
) -> anyhow::Result<()> {
    let filepath = get_file_path(key.clone(), ProverServiceDataType::FinalizationHints)
        .context("get_file_path()")?;
    tracing::info!("saving finalization hints for {:?} to: {}", key, filepath);
    let serialized = bincode::serialize(&hint).context("Failed to serialize finalization hints")?;
    fs::write(filepath, serialized).context("Failed to write finalization hints to file")
}
pub fn get_finalization_hints(
    key: ProverServiceDataKey,
) -> anyhow::Result<FinalizationHintsForProver> {
    let mut key = key;
    // For `NodeAggregation` round we have only 1 finalization hints for all circuit type.
    if key.round == AggregationRound::NodeAggregation {
        key.circuit_id = ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8;
    }
    let filepath =
        get_file_path(key, ProverServiceDataType::FinalizationHints).context("get_file_path")?;
    let file = fs::read(filepath).context("Failed to read finalization hints from file")?;
    bincode::deserialize::<FinalizationHintsForProver>(&file)
        .context("Finalization hint deserialization failed")
}

pub fn get_snark_vk(
) -> anyhow::Result<SnarkVerificationKey<Bn256, ZkSyncCircuit<Bn256, SnarkWitnessOracle<Bn256>>>> {
    let circuit_id = ZkSyncCompressionLayerStorageType::CompressionMode1Circuit as u8;
    let filepath = get_file_path(
        ProverServiceDataKey::new(circuit_id, AggregationRound::Scheduler),
        ProverServiceDataType::SnarkVerificationKey,
    )
    .context("get_file_path()")?;
    tracing::info!("Fetching verification key from path: {}", filepath);
    let text = fs::read_to_string(&filepath)
        .with_context(|| format!("Failed reading verification key from path: {filepath}"))?;
    serde_json::from_str::<
        SnarkVerificationKey<Bn256, ZkSyncCircuit<Bn256, SnarkWitnessOracle<Bn256>>>,
    >(&text)
    .with_context(|| format!("Failed deserializing verification key from path: {filepath}"))
}
