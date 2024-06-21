use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader},
};

use zksync_vm_benchmark::parse_iai::parse_iai;

fn main() {
    let [iai_before, iai_after, opcodes_before, opcodes_after] = std::env::args()
        .skip(1)
        .take(4)
        .collect::<Vec<_>>()
        .try_into()
        .expect("expected four arguments");

    let iai_before = get_name_to_cycles(&iai_before);
    let iai_after = get_name_to_cycles(&iai_after);
    let opcodes_before = get_name_to_opcodes(&opcodes_before);
    let opcodes_after = get_name_to_opcodes(&opcodes_after);

    let perf_changes = iai_before
        .keys()
        .collect::<HashSet<_>>()
        .intersection(&iai_after.keys().collect())
        .flat_map(|&name| {
            let diff = percent_difference(iai_before[name], iai_after[name]);
            if diff.abs() > 2. {
                Some((name, format!("{:+.1}%", diff)))
            } else {
                None
            }
        })
        .collect::<HashMap<_, _>>();
    let duration_changes = opcodes_before
        .keys()
        .collect::<HashSet<_>>()
        .intersection(&opcodes_after.keys().collect())
        .flat_map(|&name| {
            let opcodes_abs_diff = (opcodes_after[name] as i64) - (opcodes_before[name] as i64);

            if opcodes_abs_diff != 0 {
                Some((
                    name,
                    format!(
                        "{:+} ({:+.1}%)",
                        opcodes_abs_diff,
                        percent_difference(opcodes_before[name], opcodes_after[name])
                    ),
                ))
            } else {
                None
            }
        })
        .collect::<HashMap<_, _>>();

    let mut nonzero_diff = false;

    for name in perf_changes
        .keys()
        .collect::<HashSet<_>>()
        .union(&duration_changes.keys().collect())
    {
        // write the header before writing the first line of diff
        if !nonzero_diff {
            println!("Benchmark name | change in estimated runtime | change in number of opcodes executed \n--- | --- | ---");
            nonzero_diff = true;
        }

        let n_a = "N/A".to_string();
        println!(
            "{} | {} | {}",
            name,
            perf_changes.get(**name).unwrap_or(&n_a),
            duration_changes.get(**name).unwrap_or(&n_a),
        );
    }

    if nonzero_diff {
        println!("\n Changes in number of opcodes executed indicate that the gas price of the benchmark has changed, which causes it run out of gas at a different time. Or that it is behaving completely differently.");
    }
}

fn percent_difference(a: u64, b: u64) -> f64 {
    ((b as f64) - (a as f64)) / (a as f64) * 100.0
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
