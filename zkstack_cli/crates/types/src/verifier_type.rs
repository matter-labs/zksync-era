use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::ProverMode;

/// Selects which "main" verifier contract the ecosystem / CTM deploy scripts wire into the
/// `ChainTypeManager`. This is orthogonal to the prover mode, although the default is derived
/// from it (see [`VerifierType::resolve`]).
#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum VerifierType {
    /// Testnet (dummy) verifier that accepts any proof. Default when the prover runs in
    /// `NoProofs` mode.
    Testnet,
    /// Boojum dual verifier (FFLONK + PLONK). Default for real prover modes.
    Dual,
    /// Boojum dual verifier with the Airbender PLONK verifier wired into its third slot.
    /// Only supported on the EraVM (the ZKsyncOS dual verifier registers sub-verifiers
    /// differently), so the Airbender slot is ignored for ZKsyncOS deployments.
    Airbender,
}

impl VerifierType {
    /// Resolves the verifier type from an optional explicit override, falling back to the
    /// prover-mode-derived default (testnet for `NoProofs`, dual otherwise).
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
