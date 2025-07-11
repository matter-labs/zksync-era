use std::fmt;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_state_keeper::seal_criteria::UnexecutableReason;
use zksync_types::L2BlockNumber;

use crate::{
    metrics::AGGREGATION_METRICS,
    seal_criteria::criteria::{GasCriterion, PayloadSizeCriterion, TxCountCriterion},
};

/// Reported decision regarding block sealing.
#[derive(Debug, Clone, PartialEq)]
pub enum SealResolution {
    /// Block should not be sealed right now.
    NoSeal,
    /// Transaction should be included into the block and sealed after that.
    IncludeAndSeal,
    /// Block should be sealed and transaction should become the first tx in the next block.
    Seal,
    /// Latest transaction should be excluded from the block and become the first tx in the next block.
    /// Shouldn't be returned by ConditionalSealer. Should be obtained only from batch executor.
    ExcludeAndSeal,
    /// Unexecutable means that the transaction cannot be executed even
    /// if the block will consist of it solely. Such a transaction must be rejected.
    ///
    /// Contains a reason for why transaction was considered unexecutable.
    Unexecutable(String),
}

impl Into<zksync_state_keeper::seal_criteria::SealResolution> for SealResolution {
    fn into(self) -> zksync_state_keeper::seal_criteria::SealResolution {
        match self {
            SealResolution::NoSeal => zksync_state_keeper::seal_criteria::SealResolution::NoSeal,
            SealResolution::IncludeAndSeal => {
                zksync_state_keeper::seal_criteria::SealResolution::IncludeAndSeal
            }
            SealResolution::Seal => {
                zksync_state_keeper::seal_criteria::SealResolution::IncludeAndSeal
            }
            SealResolution::ExcludeAndSeal => {
                zksync_state_keeper::seal_criteria::SealResolution::ExcludeAndSeal
            }
            SealResolution::Unexecutable(reason) => {
                zksync_state_keeper::seal_criteria::SealResolution::Unexecutable(
                    UnexecutableReason::Other(reason),
                )
            }
        }
    }
}

impl SealResolution {
    /// Compares two seal resolutions and chooses the one that is stricter.
    /// `Unexecutable` is stricter than `ExcludeAndSeal`.
    /// `ExcludeAndSeal` is stricter than `Seal`.
    /// `Seal` is stricter than `IncludeAndSeal`.
    /// `IncludeAndSeal` is stricter than `NoSeal`.
    pub fn stricter(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unexecutable(reason), _) | (_, Self::Unexecutable(reason)) => {
                Self::Unexecutable(reason)
            }
            (Self::ExcludeAndSeal, _) | (_, Self::ExcludeAndSeal) => Self::ExcludeAndSeal,
            (Self::Seal, _) | (_, Self::Seal) => Self::Seal,
            (Self::IncludeAndSeal, _) | (_, Self::IncludeAndSeal) => Self::IncludeAndSeal,
            _ => Self::NoSeal,
        }
    }

    /// Returns `true` if block should be sealed according to this resolution.
    pub fn should_seal(&self) -> bool {
        matches!(self, Self::IncludeAndSeal | Self::Seal)
    }
}

#[derive(Debug, Default)]
pub struct SealData {
    pub payload_encoding_size: usize,
    pub gas: u64,
}

/// Checks if block should be sealed.
/// Note, that unlike pre-zkos state keeper, it doesn't require transaction to be executed.
/// Any implementation should take only resource that are known before tx execution, e.g. gas limit, tx encoding size etc.
pub trait ConditionalSealer: 'static + fmt::Debug + Send + Sync {
    /// Returns the action that should be taken by the state keeper after executing a transaction.
    #[allow(clippy::too_many_arguments)]
    fn should_seal_block(
        &self,
        block_number: L2BlockNumber,
        tx_count: usize,
        block_gas_limit: u64,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution;
}

/// Criterion that decides if block should be sealed based on some resource.
pub trait SealCriterion: fmt::Debug + Send + Sync + 'static {
    #[allow(clippy::too_many_arguments)]
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        tx_count: usize,
        block_gas_limit: u64,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution;

    // We need self here only for rust restrictions for creating an object from trait
    // https://doc.rust-lang.org/reference/items/traits.html#object-safety
    fn prom_criterion_name(&self) -> &'static str;
}

/// Implementation of [`ConditionalSealer`] used by the main node.
/// Internally uses a set of [`SealCriterion`]s to determine whether the block should be sealed.
///
/// The checks are deterministic, i.e., should depend solely on tx parameters and [`StateKeeperConfig`].
/// Non-deterministic seal criteria are expressed using [`IoSealCriterion`](super::IoSealCriterion).
#[derive(Debug, Default)]
pub struct SequencerSealer {
    config: StateKeeperConfig,
    sealers: Vec<Box<dyn SealCriterion>>,
}

impl SequencerSealer {
    pub fn new(config: StateKeeperConfig) -> Self {
        let sealers = Self::default_sealers();
        Self { config, sealers }
    }

    #[cfg(test)]
    pub(crate) fn with_sealers(
        config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        Self { config, sealers }
    }

    fn default_sealers() -> Vec<Box<dyn SealCriterion>> {
        vec![
            Box::new(GasCriterion),
            Box::new(PayloadSizeCriterion),
            Box::new(TxCountCriterion),
        ]
    }
}

impl ConditionalSealer for SequencerSealer {
    fn should_seal_block(
        &self,
        block_number: L2BlockNumber,
        tx_count: usize,
        block_gas_limit: u64,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution {
        tracing::trace!("Determining seal resolution for block #{block_number}");

        let mut final_seal_resolution = SealResolution::NoSeal;

        for sealer in &self.sealers {
            let seal_resolution =
                sealer.should_seal(&self.config, tx_count, block_gas_limit, block_data, tx_data);
            match &seal_resolution {
                SealResolution::IncludeAndSeal
                | SealResolution::Seal
                | SealResolution::Unexecutable(_) => {
                    tracing::debug!(
                        "Block #{block_number} processed by `{name}` with resolution {seal_resolution:?}",
                        name = sealer.prom_criterion_name()
                    );
                    AGGREGATION_METRICS.l1_batch_reason_inc(
                        sealer.prom_criterion_name(),
                        &seal_resolution.clone().into(),
                    );
                }
                SealResolution::NoSeal => { /* Don't do anything */ }
                SealResolution::ExcludeAndSeal => {
                    unreachable!("criterion mustn't return ExcludeAndSeal")
                }
            }

            final_seal_resolution = final_seal_resolution.stricter(seal_resolution);
        }
        final_seal_resolution
    }
}

/// Implementation of [`ConditionalSealer`] that never seals the block.
/// Can be used in contexts where, for example, state keeper configuration is not available,
/// or the decision to seal batch is taken by some other component.
#[derive(Debug)]
pub struct NoopSealer;

impl ConditionalSealer for NoopSealer {
    fn should_seal_block(
        &self,
        _block_number: L2BlockNumber,
        _tx_count: usize,
        _block_gas_limit: u64,
        _block_data: &SealData,
        _tx_data: &SealData,
    ) -> SealResolution {
        SealResolution::NoSeal
    }
}
