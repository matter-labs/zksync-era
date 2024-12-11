//! Runs all benchmarks and prints out the number of zkEVM opcodes each one executed.

use std::{collections::BTreeMap, env, fs, io, path::PathBuf};

use vm_benchmark::{CountInstructions, Fast, Legacy, BYTECODES};

#[derive(Debug)]
enum Command {
    Print,
    Diff { old: PathBuf },
}

impl Command {
    fn from_env() -> Self {
        let mut args = env::args().skip(1);
        let Some(first) = args.next() else {
            return Self::Print;
        };
        assert_eq!(first, "--diff", "Unsupported command-line arg");
        let old = args.next().expect("`--diff` requires a path to old file");
        Self::Diff { old: old.into() }
    }

    fn print_instructions(counts: &BTreeMap<&str, usize>) {
        for (bytecode_name, count) in counts {
            println!("{bytecode_name} {count}");
        }
    }

    fn parse_counts(reader: impl io::BufRead) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for line in reader.lines() {
            let line = line.unwrap();
            if line.is_empty() {
                continue;
            }
            let (name, count) = line.split_once(' ').expect("invalid output format");
            let count = count.parse().unwrap_or_else(|err| {
                panic!("invalid count for `{name}`: {err}");
            });
            counts.insert(name.to_owned(), count);
        }
        counts
    }

    fn run(self) {
        let counts: BTreeMap<_, _> = BYTECODES
            .iter()
            .map(|bytecode| {
                let tx = bytecode.deploy_tx();
                // We have a unit test comparing stats, but do it here as well just in case.
                let fast_count = Fast::count_instructions(&tx);
                let legacy_count = Legacy::count_instructions(&tx);
                assert_eq!(
                    fast_count, legacy_count,
                    "mismatch on number of instructions on bytecode `{}`",
                    bytecode.name
                );

                (bytecode.name, fast_count)
            })
            .collect();

        match self {
            Self::Print => Self::print_instructions(&counts),
            Self::Diff { old } => {
                let file = fs::File::open(&old).unwrap_or_else(|err| {
                    panic!("failed opening `{}`: {err}", old.display());
                });
                let reader = io::BufReader::new(file);
                let old_counts = Self::parse_counts(reader);

                let differing_counts: Vec<_> = counts
                    .iter()
                    .filter_map(|(&name, &new_count)| {
                        let old_count = *old_counts.get(name)?;
                        (old_count != new_count).then_some((name, old_count, new_count))
                    })
                    .collect();

                if !differing_counts.is_empty() {
                    println!("## ⚠ Detected differing instruction counts");
                    println!("| Benchmark | Old count | New count |");
                    println!("|-----------|----------:|----------:|");
                    for (name, old_count, new_count) in differing_counts {
                        println!("| {name} | {old_count} | {new_count} |");
                    }
                    println!(
                        "\nChanges in number of opcodes executed indicate that the gas price of the benchmark has changed, \
                         which causes it to run out of gas at a different time."
                    );
                }
            }
        }
    }
}

fn main() {
    Command::from_env().run();
}
