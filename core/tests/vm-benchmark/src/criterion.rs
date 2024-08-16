//! Criterion helpers and extensions.

use std::{
    cell::RefCell, convert::Infallible, env, fmt, rc::Rc, sync::Once, thread, time::Duration,
};

use criterion::{
    measurement::{Measurement, ValueFormatter, WallTime},
    Criterion, Throughput,
};
use once_cell::{sync::OnceCell as SyncOnceCell, unsync::OnceCell};
use tokio::sync::watch;
use vise::{Buckets, EncodeLabelSet, Family, Histogram, LatencyObserver, Metrics};
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
        println!("Waiting for Prometheus metrics to be pushed");
        thread::sleep(Duration::from_secs(1));
    }
}

impl PrometheusRuntime {
    fn new() -> Option<Self> {
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

thread_local! {
    static CURRENT_BENCH_LABELS: RefCell<Option<BenchLabels>> = const { RefCell::new(None) };
}
static BIN_NAME: SyncOnceCell<&'static str> = SyncOnceCell::new();

/// Measurement for criterion that exports .
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

    fn start_bench(group: String, benchmark: String, arg: Option<String>) {
        CURRENT_BENCH_LABELS.replace(Some(BenchLabels {
            bin: BIN_NAME.get().copied().unwrap_or(""),
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

    pub fn bench_metered<F>(&mut self, id: impl Into<String>, mut bench_fn: F)
    where
        F: FnMut(&mut Bencher<'_, '_>),
    {
        let id = id.into();
        MeteredTime::start_bench(self.name.clone(), id.clone(), None);
        self.inner.bench_function(id, |bencher| {
            bench_fn(&mut Bencher {
                inner: bencher,
                metrics: self.metrics,
            })
        });
    }

    pub fn bench_metered_with_input<I, F>(&mut self, id: BenchmarkId, input: &I, mut bench_fn: F)
    where
        I: ?Sized,
        F: FnMut(&mut Bencher<'_, '_>, &I),
    {
        MeteredTime::start_bench(self.name.clone(), id.benchmark, Some(id.arg));
        self.inner
            .bench_with_input(id.inner, input, |bencher, input| {
                bench_fn(
                    &mut Bencher {
                        inner: bencher,
                        metrics: self.metrics,
                    },
                    input,
                )
            });
    }
}

pub struct Bencher<'a, 'r> {
    inner: &'r mut criterion::Bencher<'a, MeteredTime>,
    metrics: &'static VmBenchmarkMetrics,
}

impl Bencher<'_, '_> {
    pub fn iter(&mut self, mut routine: impl FnMut(BenchmarkTimer)) {
        let histogram = &self.metrics.timing[&MeteredTime::get_bench()];
        self.inner.iter_custom(move |iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (measure, observation) = BenchmarkTimer::new(histogram);
                routine(measure);
                total += observation.get().copied().unwrap_or_default();
            }
            total
        })
    }
}

/// Timer for benchmarks supplied to the `Bencher::iter()` closure.
#[derive(Debug)]
#[must_use = "should be started to start measurements"]
pub struct BenchmarkTimer {
    histogram: &'static Histogram<Duration>,
    observation: Rc<OnceCell<Duration>>,
}

impl BenchmarkTimer {
    fn new(histogram: &'static Histogram<Duration>) -> (Self, Rc<OnceCell<Duration>>) {
        let observation = Rc::<OnceCell<_>>::default();
        let this = Self {
            histogram,
            observation: observation.clone(),
        };
        (this, observation)
    }

    /// Starts the timer. The timer will remain active until the returned guard is dropped. If you drop the timer implicitly,
    /// be careful with the drop order (inverse to the variable declaration order); when in doubt, drop the guard explicitly.
    pub fn start(self) -> BenchmarkTimerGuard {
        BenchmarkTimerGuard {
            observer: Some(self.histogram.start()),
            observation: self.observation,
        }
    }
}

/// Guard returned from [`BenchmarkTimer::start()`].
#[derive(Debug)]
#[must_use = "will stop the timer on drop"]
pub struct BenchmarkTimerGuard {
    observer: Option<LatencyObserver<'static>>,
    observation: Rc<OnceCell<Duration>>,
}

impl Drop for BenchmarkTimerGuard {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            let latency = observer.observe();
            self.observation.set(latency).ok();
        }
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
