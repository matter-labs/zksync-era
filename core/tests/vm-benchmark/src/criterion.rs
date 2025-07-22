//! Criterion helpers and extensions used to record benchmark timings as Prometheus metrics.

use std::{
    cell::RefCell,
    convert::Infallible,
    env, fmt, mem,
    rc::Rc,
    sync::Once,
    thread,
    time::{Duration, Instant},
};

use criterion::{
    measurement::{Measurement, ValueFormatter, WallTime},
    Criterion, Throughput,
};
use once_cell::{sync::OnceCell as SyncOnceCell, unsync::OnceCell};
use tokio::sync::watch;
use vise::{EncodeLabelSet, Family, Gauge, Metrics, Unit};
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

// We don't use histograms because benchmark results are uploaded in short bursts, which leads to missing zero values.
#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_benchmark")]
struct VmBenchmarkMetrics {
    /// Number of samples for a benchmark.
    sample_count: Family<BenchLabels, Gauge<usize>>,

    /// Mean latency for a benchmark.
    #[metrics(unit = Unit::Seconds)]
    mean_timing: Family<BenchLabels, Gauge<Duration>>,
    /// Minimum latency for a benchmark.
    #[metrics(unit = Unit::Seconds)]
    min_timing: Family<BenchLabels, Gauge<Duration>>,
    /// Maximum latency for a benchmark.
    #[metrics(unit = Unit::Seconds)]
    max_timing: Family<BenchLabels, Gauge<Duration>>,
    /// Median latency for a benchmark.
    #[metrics(unit = Unit::Seconds)]
    median_timing: Family<BenchLabels, Gauge<Duration>>,
}

#[vise::register]
static METRICS: vise::Global<VmBenchmarkMetrics> = vise::Global::new();

#[derive(Debug)]
pub struct PrometheusRuntime {
    stop_sender: watch::Sender<bool>,
    _runtime: tokio::runtime::Runtime,
}

impl Drop for PrometheusRuntime {
    fn drop(&mut self) {
        self.stop_sender.send_replace(true);
        // Metrics are pushed automatically on exit, so we wait *after* sending a stop request
        println!("Waiting for Prometheus metrics to be pushed");
        thread::sleep(Duration::from_secs(1));
    }
}

impl PrometheusRuntime {
    pub fn new() -> Option<Self> {
        const PUSH_INTERVAL: Duration = Duration::from_millis(100);

        let gateway_url = env::var("BENCHMARK_PROMETHEUS_PUSHGATEWAY_URL").ok()?;
        let runtime = tokio::runtime::Runtime::new().expect("Failed initializing Tokio runtime");
        println!("Pushing Prometheus metrics to {gateway_url} each {PUSH_INTERVAL:?}");
        let (stop_sender, stop_receiver) = watch::channel(false);
        let prometheus_config = PrometheusExporterConfig::push(gateway_url, PUSH_INTERVAL);
        runtime.spawn(prometheus_config.run(stop_receiver));
        Some(Self {
            stop_sender,
            _runtime: runtime,
        })
    }
}

/// Guard returned by [`CurrentBenchmark::set()`] that unsets the current benchmark on drop.
#[must_use = "Will unset the current benchmark when dropped"]
#[derive(Debug)]
struct CurrentBenchmarkGuard;

impl Drop for CurrentBenchmarkGuard {
    fn drop(&mut self) {
        CURRENT_BENCH.take();
    }
}

#[derive(Debug)]
struct CurrentBenchmark {
    metrics: &'static VmBenchmarkMetrics,
    labels: BenchLabels,
    observations: Vec<Duration>,
}

impl CurrentBenchmark {
    fn set(metrics: &'static VmBenchmarkMetrics, labels: BenchLabels) -> CurrentBenchmarkGuard {
        CURRENT_BENCH.replace(Some(Self {
            metrics,
            labels,
            observations: vec![],
        }));
        CurrentBenchmarkGuard
    }

    fn observe(timing: Duration) {
        CURRENT_BENCH.with_borrow_mut(|this| {
            if let Some(this) = this {
                this.observations.push(timing);
            }
        });
    }
}

impl Drop for CurrentBenchmark {
    fn drop(&mut self) {
        let mut observations = mem::take(&mut self.observations);
        if observations.is_empty() {
            return;
        }

        let len = observations.len();
        self.metrics.sample_count[&self.labels].set(len);
        let mean = observations
            .iter()
            .copied()
            .sum::<Duration>()
            .div_f32(len as f32);
        self.metrics.mean_timing[&self.labels].set(mean);

        // Could use quick median algorithm, but since there aren't that many observations expected,
        // sorting looks acceptable.
        observations.sort_unstable();
        let (min, max) = (observations[0], *observations.last().unwrap());
        self.metrics.min_timing[&self.labels].set(min);
        self.metrics.max_timing[&self.labels].set(max);
        let median = if len % 2 == 0 {
            (observations[len / 2 - 1] + observations[len / 2]) / 2
        } else {
            observations[len / 2]
        };
        self.metrics.median_timing[&self.labels].set(median);

        println!("Exported timings: min={min:?}, max={max:?}, mean={mean:?}, median={median:?}");
    }
}

thread_local! {
    static CURRENT_BENCH: RefCell<Option<CurrentBenchmark>> = const { RefCell::new(None) };
}

static BIN_NAME: SyncOnceCell<&'static str> = SyncOnceCell::new();

/// Measurement for criterion that exports timing-related metrics.
#[derive(Debug)]
pub struct MeteredTime {
    _prometheus: Option<PrometheusRuntime>,
}

impl MeteredTime {
    pub fn new(bin_name: &'static str) -> Self {
        static PROMETHEUS_INIT: Once = Once::new();

        let mut prometheus = None;
        if !is_test_mode() {
            PROMETHEUS_INIT.call_once(|| {
                prometheus = PrometheusRuntime::new();
            });
        }

        if let Err(prev_name) = BIN_NAME.set(bin_name) {
            assert_eq!(prev_name, bin_name, "attempted to redefine binary name");
        }

        Self {
            _prometheus: prometheus,
        }
    }
}

impl Measurement for MeteredTime {
    type Intermediate = Infallible;
    type Value = Duration;

    fn start(&self) -> Self::Intermediate {
        // All measurements must be done via `Bencher::iter()`
        unreachable!("must not be invoked directly");
    }

    fn end(&self, _: Self::Intermediate) -> Self::Value {
        unreachable!("must not be invoked directly");
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
    metrics: &'static VmBenchmarkMetrics,
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

    fn start_bench(&self, benchmark: String, arg: Option<String>) -> CurrentBenchmarkGuard {
        let labels = BenchLabels {
            bin: BIN_NAME.get().copied().unwrap_or(""),
            group: self.name.clone(),
            benchmark,
            arg,
        };
        CurrentBenchmark::set(self.metrics, labels)
    }

    pub fn bench_metered<F>(&mut self, id: impl Into<String>, mut bench_fn: F)
    where
        F: FnMut(&mut Bencher<'_, '_>),
    {
        let id = id.into();
        let _guard = self.start_bench(id.clone(), None);
        self.inner
            .bench_function(id, |bencher| bench_fn(&mut Bencher { inner: bencher }));
    }

    pub fn bench_metered_with_input<I, F>(&mut self, id: BenchmarkId, input: &I, mut bench_fn: F)
    where
        I: ?Sized,
        F: FnMut(&mut Bencher<'_, '_>, &I),
    {
        let _guard = self.start_bench(id.benchmark, Some(id.arg));
        self.inner
            .bench_with_input(id.inner, input, |bencher, input| {
                bench_fn(&mut Bencher { inner: bencher }, input)
            });
    }
}

pub struct Bencher<'a, 'r> {
    inner: &'r mut criterion::Bencher<'a, MeteredTime>,
}

impl Bencher<'_, '_> {
    pub fn iter(&mut self, mut routine: impl FnMut(BenchmarkTimer)) {
        self.inner.iter_custom(move |iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let timer = BenchmarkTimer::new();
                let observation = timer.observation.clone();
                routine(timer);
                let timing = observation.get().copied().unwrap_or_default();
                CurrentBenchmark::observe(timing);
                total += timing;
            }
            total
        })
    }
}

/// Timer for benchmarks supplied to the `Bencher::iter()` closure.
#[derive(Debug)]
#[must_use = "should be started to start measurements"]
pub struct BenchmarkTimer {
    observation: Rc<OnceCell<Duration>>,
}

impl BenchmarkTimer {
    fn new() -> Self {
        Self {
            observation: Rc::default(),
        }
    }

    /// Starts the timer. The timer will remain active until the returned guard is dropped. If you drop the timer implicitly,
    /// be careful with the drop order (inverse to the variable declaration order); when in doubt, drop the guard explicitly.
    pub fn start(self) -> BenchmarkTimerGuard {
        BenchmarkTimerGuard {
            started_at: Instant::now(),
            observation: self.observation,
        }
    }
}

/// Guard returned from [`BenchmarkTimer::start()`].
#[derive(Debug)]
#[must_use = "will stop the timer on drop"]
pub struct BenchmarkTimerGuard {
    started_at: Instant,
    observation: Rc<OnceCell<Duration>>,
}

impl Drop for BenchmarkTimerGuard {
    fn drop(&mut self) {
        let latency = self.started_at.elapsed();
        self.observation.set(latency).ok();
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
            metrics: &METRICS,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::BYTECODES;

    fn test_benchmark(c: &mut Criterion<MeteredTime>, metrics: &'static VmBenchmarkMetrics) {
        let mut group = c.metered_group("single");
        group.metrics = metrics;
        for bytecode in BYTECODES {
            group.bench_metered(bytecode.name, |bencher| {
                bencher.iter(|timer| {
                    let _guard = timer.start();
                    thread::sleep(Duration::from_millis(1))
                })
            });
        }
        drop(group);

        let mut group = c.metered_group("with_arg");
        group.metrics = metrics;
        for bytecode in BYTECODES {
            for arg in [1, 10, 100] {
                group.bench_metered_with_input(
                    BenchmarkId::new(bytecode.name, arg),
                    &arg,
                    |bencher, _arg| {
                        bencher.iter(|timer| {
                            let _guard = timer.start();
                            thread::sleep(Duration::from_millis(1))
                        });
                    },
                )
            }
        }
    }

    #[test]
    fn recording_benchmarks() {
        let metered_time = MeteredTime::new("test");
        let metrics = &*Box::leak(Box::<VmBenchmarkMetrics>::default());

        let mut criterion = Criterion::default()
            .warm_up_time(Duration::from_millis(10))
            .measurement_time(Duration::from_millis(10))
            .sample_size(10)
            .with_measurement(metered_time);
        test_benchmark(&mut criterion, metrics);

        let timing_labels: HashSet<_> = metrics
            .mean_timing
            .to_entries()
            .map(|(labels, _)| labels)
            .collect();
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

        // Sanity-check relations among collected metrics
        for label in &timing_labels {
            let mean = metrics.mean_timing[label].get();
            let min = metrics.min_timing[label].get();
            let max = metrics.max_timing[label].get();
            let median = metrics.median_timing[label].get();
            assert!(
                min > Duration::ZERO,
                "min={min:?}, mean={mean:?}, median = {median:?}, max={max:?}"
            );
            assert!(
                min <= mean && min <= median,
                "min={min:?}, mean={mean:?}, median = {median:?}, max={max:?}"
            );
            assert!(
                mean <= max && median <= max,
                "min={min:?}, mean={mean:?}, median = {median:?}, max={max:?}"
            );
        }
    }
}
