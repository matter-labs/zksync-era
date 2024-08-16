//! Criterion helpers and extensions.

use std::{cell::RefCell, env, fmt, sync::Once, thread, time::Duration};

use criterion::{
    measurement::{Measurement, ValueFormatter, WallTime},
    Bencher, Criterion, Throughput,
};
use tokio::sync::watch;
use vise::{Buckets, EncodeLabelSet, Family, Histogram, Metrics};
use zksync_vlog::prometheus::PrometheusExporterConfig;

/// Checks whether a benchmark binary is running in the test mode (as opposed to benchmarking).
pub fn is_test_mode() -> bool {
    !env::args().any(|arg| arg == "--bench")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
struct BenchLabels {
    bin: &'static str,
    group: String,
    benchmark: String,
    arg: Option<String>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_benchmark")]
struct VmBenchmarkMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    timing: Family<BenchLabels, Histogram<Duration>>,
}

#[vise::register]
static METRICS: vise::Global<VmBenchmarkMetrics> = vise::Global::new();

#[derive(Debug)]
struct PrometheusRuntime {
    stop_sender: watch::Sender<bool>,
    _runtime: tokio::runtime::Runtime,
}

impl Drop for PrometheusRuntime {
    fn drop(&mut self) {
        self.stop_sender.send_replace(true);
        // Metrics are pushed automatically on exit, so we wait *after* sending a stop signal
        thread::sleep(Duration::from_secs(1));
    }
}

impl PrometheusRuntime {
    fn new() -> Option<Self> {
        const PUSH_INTERVAL: Duration = Duration::from_millis(100);

        let gateway_url = env::var("BENCHMARK_PROMETHEUS_PUSHGATEWAY_URL").ok()?;
        let runtime = tokio::runtime::Runtime::new().expect("Failed initializing Tokio runtime");
        let (stop_sender, stop_receiver) = watch::channel(false);
        let prometheus_config = PrometheusExporterConfig::push(gateway_url, PUSH_INTERVAL);
        runtime.spawn(prometheus_config.run(stop_receiver));
        Some(Self {
            stop_sender,
            _runtime: runtime,
        })
    }
}

thread_local! {
    static CURRENT_BENCH_LABELS: RefCell<Option<BenchLabels>> = const { RefCell::new(None) };
}

/// Measurement for criterion that exports .
#[derive(Debug)]
pub struct MeteredTime {
    bin_name: &'static str,
    metrics: &'static VmBenchmarkMetrics,
    _prometheus: Option<PrometheusRuntime>,
}

impl MeteredTime {
    pub fn new(group_name: &'static str) -> Self {
        static PROMETHEUS_INIT: Once = Once::new();

        let mut prometheus = None;
        if !is_test_mode() {
            PROMETHEUS_INIT.call_once(|| {
                prometheus = PrometheusRuntime::new();
            });
        }

        Self {
            bin_name: group_name,
            metrics: &METRICS,
            _prometheus: prometheus,
        }
    }

    fn start_bench(group: String, benchmark: String, arg: Option<String>) {
        CURRENT_BENCH_LABELS.replace(Some(BenchLabels {
            bin: "", // will be replaced
            group,
            benchmark,
            arg,
        }));
    }

    fn get_bench() -> BenchLabels {
        CURRENT_BENCH_LABELS
            .with(|cell| cell.borrow().clone())
            .expect("current benchmark not set")
    }
}

impl Measurement for MeteredTime {
    type Intermediate = vise::LatencyObserver<'static>;
    type Value = Duration;

    fn start(&self) -> Self::Intermediate {
        let mut labels = MeteredTime::get_bench();
        labels.bin = self.bin_name;
        self.metrics.timing[&labels].start()
    }

    fn end(&self, i: Self::Intermediate) -> Self::Value {
        i.observe()
    }

    fn add(&self, v1: &Self::Value, v2: &Self::Value) -> Self::Value {
        *v1 + *v2
    }

    fn zero(&self) -> Self::Value {
        Duration::ZERO
    }

    fn to_f64(&self, value: &Self::Value) -> f64 {
        WallTime.to_f64(value)
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        WallTime.formatter()
    }
}

/// Drop-in replacement for `criterion::BenchmarkId`.
pub struct BenchmarkId {
    inner: criterion::BenchmarkId,
    benchmark: String,
    arg: String,
}

impl BenchmarkId {
    pub fn new<S: Into<String>, P: fmt::Display>(function_name: S, parameter: P) -> Self {
        let function_name = function_name.into();
        Self {
            benchmark: function_name.clone(),
            arg: parameter.to_string(),
            inner: criterion::BenchmarkId::new(function_name, parameter),
        }
    }
}

/// Drop-in replacement for `criterion::BenchmarkGroup`.
pub struct BenchmarkGroup<'a> {
    name: String,
    inner: criterion::BenchmarkGroup<'a, MeteredTime>,
}

impl BenchmarkGroup<'_> {
    pub fn sample_size(&mut self, size: usize) -> &mut Self {
        self.inner.sample_size(size);
        self
    }

    pub fn throughput(&mut self, throughput: Throughput) -> &mut Self {
        self.inner.throughput(throughput);
        self
    }

    pub fn measurement_time(&mut self, dur: Duration) -> &mut Self {
        self.inner.measurement_time(dur);
        self
    }

    pub fn bench_metered<F>(&mut self, id: impl Into<String>, bench_fn: F)
    where
        F: FnMut(&mut Bencher<'_, MeteredTime>),
    {
        let id = id.into();
        MeteredTime::start_bench(self.name.clone(), id.clone(), None);
        self.inner.bench_function(id, bench_fn);
    }

    pub fn bench_metered_with_input<I, F>(&mut self, id: BenchmarkId, input: &I, bench_fn: F)
    where
        I: ?Sized,
        F: FnMut(&mut Bencher<'_, MeteredTime>, &I),
    {
        MeteredTime::start_bench(self.name.clone(), id.benchmark, Some(id.arg));
        self.inner.bench_with_input(id.inner, input, bench_fn);
    }
}

pub trait CriterionExt {
    fn metered_group(&mut self, name: impl Into<String>) -> BenchmarkGroup<'_>;
}

impl CriterionExt for Criterion<MeteredTime> {
    fn metered_group(&mut self, name: impl Into<String>) -> BenchmarkGroup<'_> {
        let name = name.into();
        BenchmarkGroup {
            inner: self.benchmark_group(name.clone()),
            name,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::BYTECODES;

    fn test_benchmark(c: &mut Criterion<MeteredTime>) {
        let mut group = c.metered_group("single");
        for bytecode in BYTECODES {
            group.bench_metered(bytecode.name, |bencher| {
                bencher.iter(|| thread::sleep(Duration::from_millis(1)))
            });
        }
        drop(group);

        let mut group = c.metered_group("with_arg");
        for bytecode in BYTECODES {
            for arg in [1, 10, 100] {
                group.bench_metered_with_input(
                    BenchmarkId::new(bytecode.name, arg),
                    &arg,
                    |bencher, _arg| {
                        bencher.iter(|| thread::sleep(Duration::from_millis(1)));
                    },
                )
            }
        }
    }

    #[test]
    fn recording_benchmarks() {
        let mut metered_time = MeteredTime::new("test");
        let metrics = &*Box::leak(Box::<VmBenchmarkMetrics>::default());
        metered_time.metrics = metrics;

        let mut criterion = Criterion::default()
            .warm_up_time(Duration::from_millis(10))
            .measurement_time(Duration::from_millis(10))
            .sample_size(10)
            .with_measurement(metered_time);
        test_benchmark(&mut criterion);

        let timing_labels: HashSet<_> = metrics.timing.to_entries().into_keys().collect();
        // Check that labels are as expected.
        for bytecode in BYTECODES {
            assert!(timing_labels.contains(&BenchLabels {
                bin: "test",
                group: "single".to_owned(),
                benchmark: bytecode.name.to_owned(),
                arg: None,
            }));
            assert!(timing_labels.contains(&BenchLabels {
                bin: "test",
                group: "with_arg".to_owned(),
                benchmark: bytecode.name.to_owned(),
                arg: Some("1".to_owned()),
            }));
            assert!(timing_labels.contains(&BenchLabels {
                bin: "test",
                group: "with_arg".to_owned(),
                benchmark: bytecode.name.to_owned(),
                arg: Some("10".to_owned()),
            }));
            assert!(timing_labels.contains(&BenchLabels {
                bin: "test",
                group: "with_arg".to_owned(),
                benchmark: bytecode.name.to_owned(),
                arg: Some("100".to_owned()),
            }));
        }
        assert_eq!(
            timing_labels.len(),
            4 * BYTECODES.len(),
            "{timing_labels:#?}"
        );
    }
}
