use std::io::BufReader;
use vm_benchmark::parse_iai::IaiResult;

fn main() {
    let results: Vec<IaiResult> =
        vm_benchmark::parse_iai::parse_iai(BufReader::new(std::io::stdin())).collect();

    vm_benchmark::with_prometheus::with_prometheus(|| {
        for r in results {
            metrics::gauge!("vm_cachegrind.instructions", r.instructions as f64, "benchmark" => r.name.clone());
            metrics::gauge!("vm_cachegrind.l1_accesses", r.l1_accesses as f64, "benchmark" => r.name.clone());
            metrics::gauge!("vm_cachegrind.l2_accesses", r.l2_accesses as f64, "benchmark" => r.name.clone());
            metrics::gauge!("vm_cachegrind.ram_accesses", r.ram_accesses as f64, "benchmark" => r.name.clone());
            metrics::gauge!("vm_cachegrind.cycles", r.cycles as f64, "benchmark" => r.name);
        }
    })
}
