use std::io::BufReader;

use vise::{Gauge, LabeledFamily, Metrics};
use vm_benchmark::parse_iai::IaiResult;

fn main() {
    let results: Vec<IaiResult> =
        vm_benchmark::parse_iai::parse_iai(BufReader::new(std::io::stdin())).collect();

    vm_benchmark::with_prometheus::with_prometheus(|| {
        for r in results {
            VM_CACHEGRIND_METRICS.instructions[&r.name.clone()].set(r.instructions);
            VM_CACHEGRIND_METRICS.l1_accesses[&r.name.clone()].set(r.l1_accesses);
            VM_CACHEGRIND_METRICS.l2_accesses[&r.name.clone()].set(r.l2_accesses);
            VM_CACHEGRIND_METRICS.ram_accesses[&r.name.clone()].set(r.ram_accesses);
            VM_CACHEGRIND_METRICS.cycles[&r.name.clone()].set(r.cycles);
        }
    })
}

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
