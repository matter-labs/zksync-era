use std::time::{Duration, Instant};

use futures::{channel::mpsc::Receiver, StreamExt};
use operation_results_collector::OperationResultsCollector;

use crate::{
    metrics::LOADTEST_METRICS,
    report::{ActionType, Report, ReportLabel},
    report_collector::metrics_collector::MetricsCollector,
};

mod metrics_collector;
mod operation_results_collector;

/// Decision on whether loadtest considered passed or failed.
#[derive(Debug, Clone, Copy)]
pub enum LoadtestResult {
    TestPassed,
    TestFailed,
}

#[derive(Debug)]
struct Collectors {
    start: Instant,
    is_aborted: bool,
    metrics: MetricsCollector,
    operation_results: OperationResultsCollector,
}

impl Collectors {
    fn new(loadtest_duration: Duration) -> Self {
        Self {
            start: Instant::now(),
            is_aborted: false,
            metrics: MetricsCollector::default(),
            operation_results: OperationResultsCollector::new(loadtest_duration),
        }
    }

    fn report(&self, prometheus_label: String) {
        let actual_duration = self.start.elapsed();
        self.metrics.report();
        if !self.is_aborted {
            LOADTEST_METRICS.tps[&prometheus_label].set(self.operation_results.nominal_tps());
        }
        self.operation_results.report(actual_duration);
    }

    fn final_resolution(&self, expected_tx_count: Option<usize>) -> LoadtestResult {
        let is_tx_count_acceptable = expected_tx_count.is_none_or(|expected_count| {
            const MIN_ACCEPTABLE_DELTA: f64 = -10.0;
            const MAX_ACCEPTABLE_DELTA: f64 = 100.0;

            let actual_count = self.operation_results.tx_results.successes() as f64;
            let delta = 100.0 * (actual_count - expected_count as f64) / (expected_count as f64);
            tracing::info!("Expected number of processed txs: {expected_count}");
            tracing::info!("Actual number of processed txs: {actual_count}");
            tracing::info!("Delta: {delta:.1}%");

            (MIN_ACCEPTABLE_DELTA..=MAX_ACCEPTABLE_DELTA).contains(&delta)
        });

        if !is_tx_count_acceptable || self.operation_results.tx_results.failures() > 0 {
            LoadtestResult::TestFailed
        } else {
            LoadtestResult::TestPassed
        }
    }
}

/// ReportCollector is an entity capable of analyzing everything that happens in the loadtest.
///
/// It is designed to be separated from the actual execution, so that logic of the execution does not
/// interfere with the logic of analyzing, reporting and decision making.
///
/// Report collector by its nature only receives reports and uses different collectors in order to analyze them.
/// Currently, only the following collectors are used:
///
/// - MetricsCollector, which builds time distribution histograms for each kind of performed action.
/// - OperationResultsCollector, a primitive collector that counts the amount of failures and decides whether
///   test is passed.
///
/// Other possible collectors that can be implemented:
///
/// - ScriptCollector, which records all the actions (including wallet private keys and signatures), which makes it
///   possible to reproduce test once again.
/// - RetryCollector, which analyzes the average amount of retries that have to be made in order to make operation
///   succeed.
/// - PrometheusCollector, which exposes the ongoing loadtest results to grafana via prometheus.
///
/// Note that if you'll decide to implement a new collector, be sure that adding an entry in it is cheap: if you want
/// to do some IO (file or web), it's better to divide collector in two parts: an actor which receives commands through
/// a channel, and a cheap collector interface which just sends commands to the channel. This is strongly recommended,
/// since there are many reports generated during the loadtest, and otherwise it will result in the report channel
/// queue uncontrollable growth.
#[derive(Debug)]
pub struct ReportCollector {
    reports_stream: Receiver<Report>,
    expected_tx_count: Option<usize>,
    loadtest_duration: Duration,
    prometheus_label: String,
    fail_fast: bool,
}

impl ReportCollector {
    pub fn new(
        reports_stream: Receiver<Report>,
        expected_tx_count: Option<usize>,
        loadtest_duration: Duration,
        prometheus_label: String,
        fail_fast: bool,
    ) -> Self {
        Self {
            reports_stream,
            expected_tx_count,
            loadtest_duration,
            prometheus_label,
            fail_fast,
        }
    }

    pub async fn run(mut self) -> LoadtestResult {
        let mut collectors = None;
        let mut start = Instant::now();

        while let Some(report) = self.reports_stream.next().await {
            tracing::trace!("Report: {report:?}");
            if matches!(&report.action, ActionType::InitComplete) {
                assert!(collectors.is_none(), "`InitComplete` report sent twice");

                tracing::info!("Test initialization and warm-up took {:?}", start.elapsed());
                start = Instant::now();
                collectors = Some(Collectors::new(self.loadtest_duration));
                continue;
            }

            if let Some(collectors) = &mut collectors {
                if matches!(&report.label, ReportLabel::ActionDone) {
                    // We only count successfully created statistics.
                    collectors.metrics.add_metric(report.action, report.time);
                }
                collectors
                    .operation_results
                    .add_status(&report.label, report.action);

                let should_check_tx_count = matches!(report.action, ActionType::Tx(_))
                    && matches!(&report.label, ReportLabel::ActionDone);
                if should_check_tx_count {
                    let processed_tx_count = collectors.operation_results.tx_results.total();
                    if processed_tx_count % 50 == 0 {
                        let current_test_duration = start.elapsed();
                        tracing::info!(
                            "Processed {processed_tx_count} transactions after initialization, \
                             took {current_test_duration:?} (current TPS: {})",
                            collectors.operation_results.tps(current_test_duration)
                        );
                    }
                }
            }

            // Report failure, if it exists.
            if let ReportLabel::ActionFailed { error } = &report.label {
                tracing::warn!("Operation failed: {error}");
                if self.fail_fast && matches!(report.action, ActionType::Tx(_)) {
                    tracing::error!("Test aborted because of an error in transaction processing");
                    if let Some(collectors) = &mut collectors {
                        collectors.is_aborted = true;
                    }
                    break;
                }
            }
        }

        // All the receivers are gone, it's likely the end of the test.
        // Now we can output the statistics.
        if let Some(collectors) = collectors {
            collectors.report(self.prometheus_label);
            collectors.final_resolution(self.expected_tx_count)
        } else {
            tracing::error!("Test failed before initialization was completed");
            LoadtestResult::TestFailed
        }
    }
}
