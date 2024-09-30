use std::{collections::HashMap, sync::Arc};

use zksync_prover_fri_types::{
    circuit_definitions::boojum::{
        algebraic_props::{
            round_function::AbsorptionModeOverwrite, sponge::GoldilocksPoseidon2Sponge,
        },
        cs::implementations::{
            proof::Proof as CryptoProof, setup::FinalizationHintsForProver,
            transcript::GoldilocksPoisedon2Transcript,
            verifier::VerificationKey as CryptoVerificationKey,
        },
        field::goldilocks::{GoldilocksExt2, GoldilocksField},
    },
    ProverServiceDataKey,
};
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
