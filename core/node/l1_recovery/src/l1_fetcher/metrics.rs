use std::fmt;

use tokio::time::Duration;

pub const METRICS_TRACING_TARGET: &str = "metrics";

pub struct L1Metrics {
    /// The first L1 block fetched.
    pub first_l1_block_num: u64,
    /// The first L1 batch fetched.
    pub first_l1_batch_num: u64,

    /// The latest L1 block fetched.
    pub latest_l1_block_num: u64,
    /// The latest L1 batch fetched.
    pub latest_l1_batch_num: u64,

    /// The first L1 block to compare against when measuring progress.
    pub initial_l1_block: u64,
    /// The last L1 block to compare against when measuring progress.
    pub last_l1_block: u64,

    /// Time taken to procure a log from L1.
    pub log_acquisition: PerfMetric,
    /// Time taken to procure a transaction from L1.
    pub tx_acquisition: PerfMetric,
    /// Time taken to parse a [`CommitBlockInfo`] from a transaction.
    pub parsing: PerfMetric,
}

impl L1Metrics {
    pub fn new(initial_l1_block: u64) -> Self {
        L1Metrics {
            first_l1_block_num: 0,
            first_l1_batch_num: 0,
            latest_l1_block_num: 0,
            latest_l1_batch_num: 0,
            initial_l1_block,
            last_l1_block: 0,
            log_acquisition: PerfMetric::new("log_acquisition"),
            tx_acquisition: PerfMetric::new("tx_acquisition"),
            parsing: PerfMetric::new("parsing"),
        }
    }
    pub fn print(&mut self) {
        if self.latest_l1_block_num == 0 {
            return;
        }

        let progress = if let Some(total) = self.last_l1_block.checked_sub(self.initial_l1_block) {
            if total > 0 {
                let cur = self.latest_l1_block_num - self.initial_l1_block;
                // If polling past `last_l1_block`, stop at 100%.
                let perc = std::cmp::min((cur * 100) / total, 100);
                format!("{perc:>2}%")
            } else {
                " 0 ".to_string()
            }
        } else {
            // End block not initialized yet.
            " - ".to_string()
        };

        tracing::info!(
            "PROGRESS: [{}] CUR L1 BLOCK: {} L1 BATCH: {} TOTAL PROCESSED L1 BLOCKS: {} L1 BATCHES: {}",
            progress,
            self.latest_l1_block_num,
            self.latest_l1_batch_num,
            self.latest_l1_block_num - self.first_l1_block_num,
            self.latest_l1_batch_num
                .saturating_sub(self.first_l1_batch_num)
        );

        let log_acquisition = self.log_acquisition.reset();
        let tx_acquisition = self.tx_acquisition.reset();
        let parsing = self.parsing.reset();
        tracing::debug!(
            target: METRICS_TRACING_TARGET,
            "ACQUISITION: avg log {} tx {} parse {}",
            log_acquisition,
            tx_acquisition,
            parsing
        );
    }
}

/// Average, explicitly resettable time used by a specific (named) operation.
pub struct PerfMetric {
    name: String,
    total: Duration,
    count: u32,
}

impl PerfMetric {
    pub fn new(name: &str) -> Self {
        PerfMetric {
            name: String::from(name),
            total: Duration::default(),
            count: 0,
        }
    }

    pub fn add(&mut self, duration: Duration) -> u32 {
        tracing::trace!(target: METRICS_TRACING_TARGET, "{}: {:?}", self.name, duration);
        self.total += duration;
        self.count += 1;
        self.count
    }

    pub fn reset(&mut self) -> String {
        let old = format!("{}", self);
        self.total = Duration::default();
        self.count = 0;
        old
    }
}

impl fmt::Display for PerfMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.count == 0 {
            write!(f, "-")
        } else {
            let duration = self.total / self.count;
            write!(f, "{:?}", duration)
        }
    }
}
