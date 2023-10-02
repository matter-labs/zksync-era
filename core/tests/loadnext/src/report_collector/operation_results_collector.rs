use crate::report::{ActionType, ReportLabel};

use std::{fmt, time::Duration};

/// Collector that analyzes the outcomes of the performed operations.
/// Currently it's solely capable of deciding whether test was failed or not.
/// API requests are counted separately.
#[derive(Debug, Default)]
pub struct OperationResultsCollector {
    pub(super) tx_results: ResultCollector,
    api_requests_results: ResultCollector,
    subscriptions_results: ResultCollector,
    loadtest_duration: Duration,
}

#[derive(Debug, Default)]
pub(super) struct ResultCollector {
    successes: u64,
    skipped: u64,
    failures: u64,
}

impl ResultCollector {
    pub fn add_status(&mut self, status: &ReportLabel) {
        match status {
            ReportLabel::ActionDone => self.successes += 1,
            ReportLabel::ActionSkipped { .. } => self.skipped += 1,
            ReportLabel::ActionFailed { .. } => self.failures += 1,
        }
    }

    pub fn successes(&self) -> u64 {
        self.successes
    }

    pub fn failures(&self) -> u64 {
        self.failures
    }

    pub fn total(&self) -> u64 {
        self.successes + self.skipped + self.failures
    }
}

impl fmt::Display for ResultCollector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} successful, {} skipped, {} failures. {} total.",
            self.successes,
            self.skipped,
            self.failures,
            self.total()
        )
    }
}

impl OperationResultsCollector {
    pub fn new(loadtest_duration: Duration) -> Self {
        Self {
            loadtest_duration,
            ..Self::default()
        }
    }

    pub fn add_status(&mut self, status: &ReportLabel, action_type: ActionType) {
        match action_type {
            ActionType::Tx(_) => self.tx_results.add_status(status),
            ActionType::Api(_) => self.api_requests_results.add_status(status),
            ActionType::Subscription(_) => self.subscriptions_results.add_status(status),
            ActionType::InitComplete => {}
        }
    }

    pub fn tps(&self, duration: Duration) -> f64 {
        self.tx_results.successes() as f64 / duration.as_secs_f64()
    }

    pub fn nominal_tps(&self) -> f64 {
        self.tps(self.loadtest_duration)
    }

    pub fn report(&self, actual_duration: Duration) {
        tracing::info!(
            "Ran loadtest for {actual_duration:?} (requested duration: {:?}). \
             TPS: {} (with requested duration: {})",
            self.loadtest_duration,
            self.tps(actual_duration),
            self.nominal_tps()
        );
        tracing::info!("Transaction execution stats: {}", self.tx_results);
        tracing::info!("API requests stats: {}", self.api_requests_results);
        tracing::info!("Subscriptions stats: {}", self.subscriptions_results);
    }
}
