use futures::{channel::mpsc::Receiver, StreamExt};
use operation_results_collector::OperationResultsCollector;
use std::time::Duration;

use crate::report::ActionType;
use crate::{
    report::{Report, ReportLabel},
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
    metrics_collector: MetricsCollector,
    operations_results_collector: OperationResultsCollector,
    expected_tx_count: Option<usize>,
    loadtest_duration: Duration,
    prometheus_label: String,
}

impl ReportCollector {
    pub fn new(
        reports_stream: Receiver<Report>,
        expected_tx_count: Option<usize>,
        loadtest_duration: Duration,
        prometheus_label: String,
    ) -> Self {
        Self {
            reports_stream,
            metrics_collector: MetricsCollector::new(),
            operations_results_collector: OperationResultsCollector::new(loadtest_duration),
            expected_tx_count,
            loadtest_duration,
            prometheus_label,
        }
    }

    pub async fn run(mut self) -> LoadtestResult {
        while let Some(report) = self.reports_stream.next().await {
            vlog::trace!("Report: {:?}", &report);
            if matches!(&report.action, ActionType::InitComplete) {
                self.metrics_collector = MetricsCollector::new();
                self.operations_results_collector =
                    OperationResultsCollector::new(self.loadtest_duration);
                continue;
            }

            if matches!(&report.label, ReportLabel::ActionDone) {
                // We only count successfully created statistics.
                self.metrics_collector
                    .add_metric(report.action, report.time);
            }

            self.operations_results_collector
                .add_status(&report.label, report.action);

            // Report failure, if it exists.
            if let ReportLabel::ActionFailed { error } = &report.label {
                vlog::warn!("Operation failed: {}", error);
            }
        }

        // All the receivers are gone, it's likely the end of the test.
        // Now we can output the statistics.
        self.metrics_collector.report();
        metrics::gauge!(
            "loadtest.tps",
            self.operations_results_collector.tps(),
            "label" => self.prometheus_label.clone(),
        );
        self.operations_results_collector.report();

        self.final_resolution()
    }

    fn final_resolution(&self) -> LoadtestResult {
        let is_tx_count_acceptable = if let Some(expected_tx_count) = self.expected_tx_count {
            const MIN_ACCEPTABLE_DELTA: f64 = -10.0;
            const MAX_ACCEPTABLE_DELTA: f64 = 100.0;

            let actual_count = self.operations_results_collector.tx_results.successes() as f64;
            let delta =
                100.0 * (actual_count - expected_tx_count as f64) / (expected_tx_count as f64);
            vlog::info!("Expected number of processed txs: {}", expected_tx_count);
            vlog::info!("Actual number of processed txs: {}", actual_count);
            vlog::info!("Delta: {:.1}%", delta);

            (MIN_ACCEPTABLE_DELTA..=MAX_ACCEPTABLE_DELTA).contains(&delta)
        } else {
            true
        };

        if !is_tx_count_acceptable || self.operations_results_collector.tx_results.failures() > 0 {
            LoadtestResult::TestFailed
        } else {
            LoadtestResult::TestPassed
        }
    }
}
