use serde::{Deserialize, Serialize};
#[cfg(feature = "gpu-light")]
use shivini::cs::GpuSetup;
use zkevm_test_harness::compute_setups::CircuitSetupData;
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

impl From<CircuitSetupData> for GoldilocksProverSetupData {
    fn from(circuit_setup_data: CircuitSetupData) -> Self {
        Self {
            setup_base: circuit_setup_data.setup_base,
            setup: circuit_setup_data.setup,
            vk: circuit_setup_data.vk,
            setup_tree: circuit_setup_data.setup_tree,
            vars_hint: circuit_setup_data.vars_hint,
            wits_hint: circuit_setup_data.wits_hint,
            finalization_hint: circuit_setup_data.finalization_hint,
        }
    }
}

#[cfg(feature = "gpu-light")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound = "F: serde::Serialize + serde::de::DeserializeOwned")]
pub struct GpuProverSetupData<F: PrimeField + SmallField, H: shivini::GpuTreeHasher + TreeHasher<F>>
{
    pub setup: GpuSetup<H>,
    #[serde(bound(
        serialize = "<H as TreeHasher<F>>::Output: serde::Serialize",
        deserialize = "<H as TreeHasher<F>>::Output: serde::de::DeserializeOwned"
    ))]
    pub vk: VerificationKey<F, H>,
    pub finalization_hint: FinalizationHintsForProver,
}

#[cfg(feature = "gpu-light")]
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