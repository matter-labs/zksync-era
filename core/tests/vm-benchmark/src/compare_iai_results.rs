use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
};

use vm_benchmark::parse_iai::parse_iai;

fn main() {
    let [iai_before, iai_after, opcodes_before, opcodes_after] = std::env::args()
        .skip(1)
        .take(2)
        .collect::<Vec<_>>()
        .try_into()
        .expect("expected four arguments");

    let iai_before = get_name_to_cycles(&iai_before);
    let iai_after = get_name_to_cycles(&iai_after);
    let opcodes_before = get_name_to_opcodes(&opcodes_before);
    let opcodes_after = get_name_to_opcodes(&opcodes_after);

    let mut nonzero_diff = false;

    for (name, cycles) in iai_before {
        if let Some(&cycles2) = iai_after.get(&name) {
            let cycles_diff = ((cycles2 as f64) - (cycles as f64)) / (cycles as f64);
            let opcodes_diff = opcodes_after.get(&name).cloned().unwrap_or_default()
                - opcodes_before.get(&name).cloned().unwrap_or_default();
            if cycles_diff.abs() > 0.02 {
                // write the header before writing the first line of diff
                if !nonzero_diff {
                    println!("Benchmark name | change in estimated runtime | change in number of opcodes executed \n--- | --- | ---");
                    nonzero_diff = true;
                }

                println!(
                    "{} | {:+.1}% | {:+}",
                    name,
                    cycles_diff * 100.0,
                    opcodes_diff
                );
            }
        }
    }

    if nonzero_diff {
        println!("\n Changes in number of opcodes executed indicate that the gas price of the benchmark has changed, which causes it run out of gas at a different time. Or that it is behaving completely differently.");
    }
}

fn get_name_to_cycles(filename: &str) -> HashMap<String, u64> {
    parse_iai(BufReader::new(
        File::open(filename).expect("failed to open file"),
    ))
    .map(|x| (x.name, x.cycles))
    .collect()
}

fn get_name_to_opcodes(filename: &str) -> HashMap<String, u64> {
    BufReader::new(File::open(filename).expect("failed to open file"))
        .lines()
        .map(|line| {
            let line = line.unwrap();
            let mut it = line.split_whitespace();
            (
                it.next().unwrap().to_string(),
                it.next().unwrap().parse().unwrap(),
            )
        })
        .collect()
}
