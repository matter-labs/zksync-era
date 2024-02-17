#![feature(allocator_api)]

use serde::{Deserialize, Serialize};
use zksync_prover_fri_types::circuit_definitions::boojum::{
    algebraic_props::{round_function::AbsorptionModeOverwrite, sponge::GenericAlgebraicSponge},
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
        goldilocks::GoldilocksField, traits::field_like::PrimeFieldLikeVectorized, PrimeField,
        SmallField,
    },
    implementations::poseidon2::Poseidon2Goldilocks,
};
#[cfg(feature = "gpu")]
use {shivini::cs::GpuSetup, std::alloc::Global};

pub mod commitment_utils;
pub mod keystore;
pub mod setup_data_generator;
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
