use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::ProverMode;

/// Which "main" verifier contract the ecosystem / CTM deploy scripts wire into the
/// `ChainTypeManager`.
#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum VerifierType {
    /// Testnet (dummy) verifier that accepts any proof. Default for `NoProofs` prover mode.
    Testnet,
    /// Boojum dual verifier (FFLONK + PLONK). Default for real prover modes.
    Dual,
    /// Dual verifier with the Airbender PLONK verifier wired into its third slot.
    /// EraVM only; ignored for ZKsyncOS deployments.
    Airbender,
}

impl VerifierType {
    /// Explicit override, or the prover-mode-derived default.
    pub fn resolve(explicit: Option<VerifierType>, prover_mode: ProverMode) -> Self {
        explicit.unwrap_or(match prover_mode {
            ProverMode::NoProofs => VerifierType::Testnet,
            _ => VerifierType::Dual,
        })
    }

    /// Whether the testnet (dummy) verifier should be deployed as the main verifier.
    pub fn is_testnet(self) -> bool {
        matches!(self, VerifierType::Testnet)
    }

    /// Whether the Airbender PLONK verifier should be deployed and wired into the dual verifier.
    pub fn is_airbender(self) -> bool {
        matches!(self, VerifierType::Airbender)
    }
}
