//! Measures the number of host instructions required to run the benchmark bytecodes.

use std::{env, sync::mpsc};

use vise::{Gauge, LabeledFamily, Metrics};
use vm_benchmark::{
    criterion::PrometheusRuntime, BenchmarkingVm, BenchmarkingVmFactory, Fast, FastNoSignatures,
    Legacy, BYTECODES,
};
use yab::{
    reporter::{BenchmarkOutput, BenchmarkReporter, Reporter},
    AccessSummary, BenchMode, Bencher, BenchmarkId,
};

fn benchmarks_for_vm<VM: BenchmarkingVmFactory>(bencher: &mut Bencher) {
    bencher.bench(
        BenchmarkId::new("init", VM::LABEL.as_str()),
        BenchmarkingVm::<VM>::default,
    );

    for bytecode in BYTECODES {
        bencher.bench_with_capture(
            BenchmarkId::new(bytecode.name, VM::LABEL.as_str()),
            |capture| {
                let mut vm = yab::black_box(BenchmarkingVm::<VM>::default());
                let tx = yab::black_box(bytecode.deploy_tx());
                capture.measure(|| vm.run_transaction(&tx));
            },
        );
    }
}

/// Reporter that pushes cachegrind metrics to Prometheus.
#[derive(Debug)]
struct MetricsReporter {
    _runtime: Option<PrometheusRuntime>,
}

impl Default for MetricsReporter {
    fn default() -> Self {
        Self {
            _runtime: PrometheusRuntime::new(),
        }
    }
}

impl Reporter for MetricsReporter {
    fn new_benchmark(&mut self, id: &BenchmarkId) -> Box<dyn BenchmarkReporter> {
        Box::new(MetricsBenchmarkReporter(id.clone()))
    }
}

#[derive(Debug)]
struct MetricsBenchmarkReporter(BenchmarkId);

impl BenchmarkReporter for MetricsBenchmarkReporter {
    fn ok(self: Box<Self>, output: &BenchmarkOutput) {
        #[derive(Debug, Metrics)]
        #[metrics(prefix = "vm_cachegrind")]
        struct VmCachegrindMetrics {
            #[metrics(labels = ["benchmark"])]
            instructions: LabeledFamily<String, Gauge<u64>>,
            #[metrics(labels = ["benchmark"])]
            l1_accesses: LabeledFamily<String, Gauge<u64>>,
            #[metrics(labels = ["benchmark"])]
            l2_accesses: LabeledFamily<String, Gauge<u64>>,
            #[metrics(labels = ["benchmark"])]
            ram_accesses: LabeledFamily<String, Gauge<u64>>,
            #[metrics(labels = ["benchmark"])]
            cycles: LabeledFamily<String, Gauge<u64>>,
        }

        #[vise::register]
        static VM_CACHEGRIND_METRICS: vise::Global<VmCachegrindMetrics> = vise::Global::new();

        let id = self.0.to_string();
        VM_CACHEGRIND_METRICS.instructions[&id].set(output.stats.total_instructions());
        if let Some(&full) = output.stats.as_full() {
            let summary = AccessSummary::from(full);
            VM_CACHEGRIND_METRICS.l1_accesses[&id].set(summary.l1_hits);
            VM_CACHEGRIND_METRICS.l2_accesses[&id].set(summary.l3_hits);
            VM_CACHEGRIND_METRICS.ram_accesses[&id].set(summary.ram_accesses);
            VM_CACHEGRIND_METRICS.cycles[&id].set(summary.estimated_cycles());
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Comparison {
    current_cycles: u64,
    prev_cycles: Option<u64>,
}

impl Comparison {
    fn percent_difference(a: u64, b: u64) -> f64 {
        ((b as i64) - (a as i64)) as f64 / (a as f64) * 100.0
    }

    fn new(output: &BenchmarkOutput) -> Option<Self> {
        let current_cycles = AccessSummary::from(*output.stats.as_full()?).estimated_cycles();
        let prev_cycles = if let Some(prev_stats) = &output.prev_stats {
            Some(AccessSummary::from(*prev_stats.as_full()?).estimated_cycles())
        } else {
            None
        };

        Some(Self {
            current_cycles,
            prev_cycles,
        })
    }

    fn cycles_diff(&self) -> Option<f64> {
        self.prev_cycles
            .map(|prev_cycles| Self::percent_difference(prev_cycles, self.current_cycles))
    }
}

/// Reporter that outputs diffs in a Markdown table to stdout after all benchmarks are completed.
///
/// Significant diff level can be changed via `BENCHMARK_DIFF_THRESHOLD_PERCENT` env var; it is set to 1% by default.
#[derive(Debug)]
struct ComparisonReporter {
    comparisons_sender: mpsc::Sender<(String, Comparison)>,
    comparisons_receiver: mpsc::Receiver<(String, Comparison)>,
}

impl Default for ComparisonReporter {
    fn default() -> Self {
        let (comparisons_sender, comparisons_receiver) = mpsc::channel();
        Self {
            comparisons_sender,
            comparisons_receiver,
        }
    }
}

impl Reporter for ComparisonReporter {
    fn new_benchmark(&mut self, id: &BenchmarkId) -> Box<dyn BenchmarkReporter> {
        Box::new(BenchmarkComparison {
            comparisons_sender: self.comparisons_sender.clone(),
            id: id.clone(),
        })
    }

    fn ok(self: Box<Self>) {
        const ENV_VAR: &str = "BENCHMARK_DIFF_THRESHOLD_PERCENT";

        let diff_threshold = env::var(ENV_VAR).unwrap_or_else(|_| "1.0".into());
        let diff_threshold: f64 = diff_threshold.parse().unwrap_or_else(|err| {
            panic!("incorrect `{ENV_VAR}` value: {err}");
        });

        // Drop the sender to not hang on the iteration below.
        drop(self.comparisons_sender);
        let mut comparisons: Vec<_> = self.comparisons_receiver.iter().collect();
        comparisons.retain(|(_, diff)| {
            // Output all stats if `diff_threshold <= 0.0` since this is what the user expects
            diff.cycles_diff().unwrap_or(0.0) >= diff_threshold
        });
        if comparisons.is_empty() {
            return;
        }

        comparisons.sort_unstable_by(|(name, _), (other_name, _)| name.cmp(other_name));

        println!("\n## Detected VM performance changes");
        println!("Benchmark name | Est. cycles | Change in est. cycles |");
        println!("|:---|---:|---:|");
        for (name, comparison) in &comparisons {
            let diff = comparison
                .cycles_diff()
                .map_or_else(|| "N/A".to_string(), |diff| format!("{diff:+.1}%"));
            println!("| {name} | {} | {diff} |", comparison.current_cycles);
        }
    }
}

#[derive(Debug)]
struct BenchmarkComparison {
    comparisons_sender: mpsc::Sender<(String, Comparison)>,
    id: BenchmarkId,
}

impl BenchmarkReporter for BenchmarkComparison {
    fn ok(self: Box<Self>, output: &BenchmarkOutput) {
        if let Some(diff) = Comparison::new(output) {
            self.comparisons_sender
                .send((self.id.to_string(), diff))
                .ok();
        }
    }
}

fn benchmarks(bencher: &mut Bencher) {
    if bencher.mode() == BenchMode::PrintResults {
        // Only customize reporting if outputting previously collected benchmark result in order to prevent
        // reporters influencing cachegrind stats.
        bencher
            .add_reporter(MetricsReporter::default())
            .add_reporter(ComparisonReporter::default());
    }
    benchmarks_for_vm::<Fast>(bencher);
    benchmarks_for_vm::<FastNoSignatures>(bencher);
    benchmarks_for_vm::<Legacy>(bencher);
}

yab::main!(benchmarks);
