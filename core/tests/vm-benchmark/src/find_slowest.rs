use std::{
    io::Write,
    time::{Duration, Instant},
};
use vm_benchmark_harness::*;

fn main() {
    let mut results = vec![];

    let arg = std::env::args()
        .nth(1)
        .expect("Expected directory of contracts to rank as first argument.");
    let files = std::fs::read_dir(arg).expect("Failed to list dir");

    let mut last_progress_update = Instant::now();

    for (i, file) in files.enumerate() {
        let path = file.unwrap().path();

        let test_contract = std::fs::read(&path).expect("failed to read file");

        if let Some(code) = cut_to_allowed_bytecode_size(&test_contract) {
            let tx = get_deploy_tx(code);

            let start_time = Instant::now();
            BenchmarkingVm::new().run_transaction(&tx);
            results.push((start_time.elapsed(), path));
        }

        if last_progress_update.elapsed() > Duration::from_millis(100) {
            print!("\r{}", i);
            std::io::stdout().flush().unwrap();
            last_progress_update = Instant::now();
        }
    }
    println!();

    results.sort();
    for (time, path) in results.iter().rev().take(30) {
        println!("{} took {:?}", path.display(), time);
    }
}
