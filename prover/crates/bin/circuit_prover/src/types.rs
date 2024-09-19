use std::collections::HashMap;
use std::sync::Arc;

use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::round_function::AbsorptionModeOverwrite;
use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::sponge::GoldilocksPoseidon2Sponge;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::transcript::GoldilocksPoisedon2Transcript;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::{
    proof::Proof as CryptoProof, verifier::VerificationKey as CryptoVerificationKey,
};
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::{
    GoldilocksExt2, GoldilocksField,
};
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_prover_keystore::GoldilocksGpuProverSetupData;

// prover types
pub type DefaultTranscript = GoldilocksPoisedon2Transcript;
pub type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

type F = GoldilocksField;
type H = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
type Ext = GoldilocksExt2;
pub type Proof = CryptoProof<F, H, Ext>;
pub type VerificationKey = CryptoVerificationKey<F, H>;

// cache types
pub type SetupDataCache = HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>;
pub type FinalizationHintsCache = HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>;
