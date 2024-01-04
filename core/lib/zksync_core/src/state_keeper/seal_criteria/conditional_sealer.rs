//! This module represents the conditional sealer, which can decide whether the batch
//! should be sealed after executing a particular transaction.
//!
//! The conditional sealer abstraction allows to implement different sealing strategies, e.g. the actual
//! sealing strategy for the main node or noop sealer for the external node.

use std::fmt;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::ProtocolVersionId;

use super::{criteria, SealCriterion, SealData, SealResolution, AGGREGATION_METRICS};

/// Checks if an L1 batch should be sealed after executing a transaction.
pub trait ConditionalSealer: 'static + fmt::Debug + Send + Sync {
    /// Finds a reason why a transaction with the specified `data` is unexecutable.
    ///
    /// Can be used to determine whether the transaction can be executed by the sequencer.
    fn find_unexecutable_reason(
        &self,
        data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<&'static str>;

    /// Returns the action that should be taken by the state keeper after executing a transaction.
    fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution;
}

/// Implementation of [`ConditionalSealer`] used by the main node.
/// Internally uses a set of [`SealCriterion`]s to determine whether the batch should be sealed.
///
/// The checks are deterministic, i.e., should depend solely on execution metrics and [`StateKeeperConfig`].
/// Non-deterministic seal criteria are expressed using [`IoSealCriteria`](super::IoSealCriteria).
#[derive(Debug)]
pub struct SequencerSealer {
    config: StateKeeperConfig,
    sealers: Vec<Box<dyn SealCriterion>>,
}

impl ConditionalSealer for SequencerSealer {
    fn find_unexecutable_reason(
        &self,
        data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<&'static str> {
        for sealer in &self.sealers {
            const MOCK_BLOCK_TIMESTAMP: u128 = 0;
            const TX_COUNT: usize = 1;

            let resolution = sealer.should_seal(
                &self.config,
                MOCK_BLOCK_TIMESTAMP,
                TX_COUNT,
                data,
                data,
                protocol_version,
            );
            if matches!(resolution, SealResolution::Unexecutable(_)) {
                return Some(sealer.prom_criterion_name());
            }
        }
        None
    }

    fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        tracing::trace!(
            "Determining seal resolution for L1 batch #{l1_batch_number} with {tx_count} transactions \
             and metrics {:?}",
            block_data.execution_metrics
        );

        let mut final_seal_resolution = SealResolution::NoSeal;
        for sealer in &self.sealers {
            let seal_resolution = sealer.should_seal(
                &self.config,
                block_open_timestamp_ms,
                tx_count,
                block_data,
                tx_data,
                protocol_version,
            );
            match &seal_resolution {
                SealResolution::IncludeAndSeal
                | SealResolution::ExcludeAndSeal
                | SealResolution::Unexecutable(_) => {
                    tracing::debug!(
                        "L1 batch #{l1_batch_number} processed by `{name}` with resolution {seal_resolution:?}",
                        name = sealer.prom_criterion_name()
                    );
                    AGGREGATION_METRICS.inc(sealer.prom_criterion_name(), &seal_resolution);
                }
                SealResolution::NoSeal => { /* Don't do anything */ }
            }

            final_seal_resolution = final_seal_resolution.stricter(seal_resolution);
        }
        final_seal_resolution
    }
}

impl SequencerSealer {
    pub(crate) fn new(config: StateKeeperConfig) -> Self {
        let sealers = Self::default_sealers();
        Self { config, sealers }
    }

    #[cfg(test)]
    pub(in crate::state_keeper) fn with_sealers(
        config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        Self { config, sealers }
    }

    fn default_sealers() -> Vec<Box<dyn SealCriterion>> {
        vec![
            Box::new(criteria::SlotsCriterion),
            Box::new(criteria::GasCriterion),
            Box::new(criteria::PubDataBytesCriterion),
            Box::new(criteria::CircuitsCriterion),
            Box::new(criteria::TxEncodingSizeCriterion),
        ]
    }
}

/// Implementation of [`ConditionalSealer`] that never seals the batch.
/// Can be used in contexts where, for example, state keeper configuration is not available,
/// or the decision to seal batch is taken by some other component.
#[derive(Debug)]
pub struct NoopSealer;

impl ConditionalSealer for NoopSealer {
    fn find_unexecutable_reason(
        &self,
        _data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Option<&'static str> {
        None
    }

    fn should_seal_l1_batch(
        &self,
        _l1_batch_number: u32,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        SealResolution::NoSeal
    }
}
