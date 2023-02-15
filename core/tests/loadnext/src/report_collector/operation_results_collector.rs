use crate::report::{ActionType, ReportLabel};

/// Collector that analyzes the outcomes of the performed operations.
/// Currently it's solely capable of deciding whether test was failed or not.
/// API requests are counted separately.
#[derive(Debug, Clone, Default)]
pub struct OperationResultsCollector {
    pub(super) tx_results: ResultCollector,
    api_requests_results: ResultCollector,
    subscriptions_results: ResultCollector,
    explorer_api_requests_results: ResultCollector,
}

#[derive(Debug, Clone, Default)]
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

    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    pub fn failures(&self) -> u64 {
        self.failures
    }

    pub fn total(&self) -> u64 {
        self.successes + self.skipped + self.failures
    }
}

impl OperationResultsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_status(&mut self, status: &ReportLabel, action_type: ActionType) {
        match action_type {
            ActionType::Tx(_) => self.tx_results.add_status(status),
            ActionType::Api(_) => self.api_requests_results.add_status(status),
            ActionType::Subscription(_) => self.subscriptions_results.add_status(status),
            ActionType::ExplorerApi(_) => self.explorer_api_requests_results.add_status(status),
        }
    }

    pub fn report(&self) {
        vlog::info!(
            "Loadtest status: {} successful operations, {} skipped, {} failures. {} actions total.",
            self.tx_results.successes(),
            self.tx_results.skipped(),
            self.tx_results.failures(),
            self.tx_results.total()
        );
        vlog::info!(
            "API requests stats: {} successful, {} skipped, {} failures. {} total. ",
            self.api_requests_results.successes(),
            self.api_requests_results.skipped(),
            self.api_requests_results.failures(),
            self.api_requests_results.total()
        );
        vlog::info!(
            "Subscriptions stats: {} successful, {} skipped, {} failures. {} total. ",
            self.subscriptions_results.successes(),
            self.subscriptions_results.skipped(),
            self.subscriptions_results.failures(),
            self.subscriptions_results.total()
        );
        vlog::info!(
            "Explorer api stats: {} successful, {} skipped, {} failures. {} total. ",
            self.explorer_api_requests_results.successes(),
            self.explorer_api_requests_results.skipped(),
            self.explorer_api_requests_results.failures(),
            self.explorer_api_requests_results.total()
        );
    }
}
