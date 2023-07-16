use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use vm_benchmark::parse_iai::parse_iai;

fn main() {
    let args: [String; 2] = std::env::args()
        .skip(1)
        .take(2)
        .collect::<Vec<_>>()
        .try_into()
        .expect("expected two arguments");

    let before = get_name_to_cycles(&args[0]);
    let after = get_name_to_cycles(&args[1]);

    let mut header_written = false;

    for (name, cycles) in before {
        if let Some(&cycles2) = after.get(&name) {
            let change = ((cycles2 as f64) - (cycles as f64)) / (cycles as f64);
            if change.abs() > 0.02 {
                if !header_written {
                    println!("Benchmark name | Difference in runtime\n--- | ---");
                    header_written = true;
                }

                println!("{} | {:+.1}%", name, change * 100.0);
            }
        }
    }
}

fn get_name_to_cycles(filename: &str) -> HashMap<String, u64> {
    parse_iai(BufReader::new(
        File::open(filename).expect("failed to open file"),
    ))
    .map(|x| (x.name, x.cycles))
    .collect()
}
