use std::{env, io::BufReader, time::Duration};

use tokio::sync::watch;
use vise::{Gauge, LabeledFamily, Metrics};
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::common::{parse_iai, IaiResult};

mod common;

#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_cachegrind")]
pub(crate) struct VmCachegrindMetrics {
    #[metrics(labels = ["benchmark"])]
    pub instructions: LabeledFamily<String, Gauge<u64>>,
    #[metrics(labels = ["benchmark"])]
    pub l1_accesses: LabeledFamily<String, Gauge<u64>>,
    #[metrics(labels = ["benchmark"])]
    pub l2_accesses: LabeledFamily<String, Gauge<u64>>,
    #[metrics(labels = ["benchmark"])]
    pub ram_accesses: LabeledFamily<String, Gauge<u64>>,
    #[metrics(labels = ["benchmark"])]
    pub cycles: LabeledFamily<String, Gauge<u64>>,
}

#[vise::register]
pub(crate) static VM_CACHEGRIND_METRICS: vise::Global<VmCachegrindMetrics> = vise::Global::new();

#[tokio::main]
async fn main() {
    let results: Vec<IaiResult> = parse_iai(BufReader::new(std::io::stdin())).collect();

    let endpoint = env::var("BENCHMARK_PROMETHEUS_PUSHGATEWAY_URL")
        .expect("`BENCHMARK_PROMETHEUS_PUSHGATEWAY_URL` env var is not set");
    let (stop_sender, stop_receiver) = watch::channel(false);
    let prometheus_config =
        PrometheusExporterConfig::push(endpoint.to_owned(), Duration::from_millis(100));
    tokio::spawn(prometheus_config.run(stop_receiver));

    for result in results {
        let name = result.name;
        VM_CACHEGRIND_METRICS.instructions[&name.clone()].set(result.instructions);
        VM_CACHEGRIND_METRICS.l1_accesses[&name.clone()].set(result.l1_accesses);
        VM_CACHEGRIND_METRICS.l2_accesses[&name.clone()].set(result.l2_accesses);
        VM_CACHEGRIND_METRICS.ram_accesses[&name.clone()].set(result.ram_accesses);
        VM_CACHEGRIND_METRICS.cycles[&name].set(result.cycles);
    }

    println!("Waiting for push to happen...");
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop_sender.send_replace(true);
}
