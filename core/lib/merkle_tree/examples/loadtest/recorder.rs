//! Simple `metrics::Recorder` implementation that prints information to stdout.

use metrics::{
    Counter, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Label, Recorder, SharedString,
    Unit,
};

use std::{
    collections::HashMap,
    fmt::{self, Write as _},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

type SharedMetadata = Mutex<HashMap<KeyName, MetricMetadata>>;

#[derive(Debug, Clone, Copy)]
enum MetricKind {
    Gauge,
    Histogram,
}

impl fmt::Display for MetricKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
        })
    }
}

#[derive(Debug)]
struct PrintingMetric {
    kind: MetricKind,
    key: KeyName,
    labels: Vec<Label>,
    value: AtomicU64,
    unit: Option<Unit>,
}

impl PrintingMetric {
    fn new(kind: MetricKind, key: KeyName, labels: Vec<Label>, unit: Option<Unit>) -> Self {
        Self {
            kind,
            key,
            labels,
            value: AtomicU64::new(0),
            unit,
        }
    }

    fn report_value(&self) {
        let value = f64::from_bits(self.value.load(Ordering::Relaxed));
        let labels = if self.labels.is_empty() {
            String::new()
        } else {
            let mut labels = "{".to_string();
            for (i, label) in self.labels.iter().enumerate() {
                write!(labels, "{}={}", label.key(), label.value()).unwrap();
                if i + 1 < self.labels.len() {
                    labels.push_str(", ");
                }
            }
            labels.push('}');
            labels
        };

        let unit = match &self.unit {
            None | Some(Unit::Count) => "",
            Some(other) => other.as_str(),
        };
        let space = if unit.is_empty() { "" } else { " " };
        println!(
            "[{kind}] {key}{labels} = {value}{space}{unit}",
            kind = self.kind,
            labels = labels,
            key = self.key.as_str()
        );
    }
}

impl GaugeFn for PrintingMetric {
    fn increment(&self, value: f64) {
        self.value.increment(value);
        self.report_value();
        // ^ These calls are non-atomic, but in practice values are updated infrequently,
        // so we're OK with it.
    }

    fn decrement(&self, value: f64) {
        self.value.decrement(value);
        self.report_value();
    }

    fn set(&self, value: f64) {
        self.value.set(value);
        self.report_value();
    }
}

impl HistogramFn for PrintingMetric {
    fn record(&self, value: f64) {
        self.set(value);
    }
}

#[derive(Debug, Default)]
struct MetricMetadata {
    unit: Option<Unit>,
}

#[derive(Debug, Default)]
pub struct PrintingRecorder {
    metadata: SharedMetadata,
}

impl PrintingRecorder {
    pub fn install() {
        let this = Self::default();
        metrics::set_boxed_recorder(Box::new(this))
            .expect("failed setting printing metrics recorder")
    }

    fn create_metric(&self, kind: MetricKind, key: &Key) -> Arc<PrintingMetric> {
        let (key_name, labels) = key.clone().into_parts();
        let mut metadata = self.metadata.lock().unwrap();
        let metadata = metadata.entry(key_name.clone()).or_default();
        let gauge = PrintingMetric::new(kind, key_name, labels, metadata.unit);
        Arc::new(gauge)
    }
}

impl Recorder for PrintingRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, _description: SharedString) {
        let mut metadata = self.metadata.lock().unwrap();
        let metadata = metadata.entry(key).or_default();
        metadata.unit = unit.or(metadata.unit);
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.describe_counter(key, unit, description);
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.describe_counter(key, unit, description);
    }

    fn register_counter(&self, _key: &Key) -> Counter {
        Counter::noop() // counters are not used
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        Gauge::from_arc(self.create_metric(MetricKind::Gauge, key))
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        Histogram::from_arc(self.create_metric(MetricKind::Histogram, key))
    }
}
