use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self},
};

use clap::Parser;
use glob::glob;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

#[derive(Debug, Serialize, Deserialize)]
struct ABIEntry {
    #[serde(rename = "type")]
    entry_type: String,
    name: Option<String>,
    inputs: Option<Vec<ABIInput>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ABIInput {
    #[serde(rename = "type")]
    input_type: String,
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    contracts_dir: String,
    output_file: String,
}

/// Computes solidity selector for a given method and arguments.
fn compute_selector(name: &str, inputs: &[ABIInput]) -> String {
    let signature = format!(
        "{}({})",
        name,
        inputs
            .iter()
            .map(|i| i.input_type.clone())
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut hasher = Keccak256::new();
    hasher.update(signature);
    format!("{:x}", hasher.finalize())[..8].to_string()
}

/// Analyses all the JSON files, looking for 'abi' entries, and then computing the selectors for them.
fn process_files(directory: &str, output_file: &str) -> io::Result<()> {
    let mut selectors: HashMap<String, String> = match File::open(output_file) {
        Ok(file) => serde_json::from_reader(file).unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    let selectors_before = selectors.len();
    let mut analyzed_files = 0;

    for entry in glob(&format!("{}/**/*.json", directory)).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                let file_path = path.clone();
                let file = File::open(path)?;
                let json: Result<serde_json::Value, _> = serde_json::from_reader(file);

                if let Ok(json) = json {
                    if let Some(abi) = json.get("abi").and_then(|v| v.as_array()) {
                        analyzed_files += 1;
                        for item in abi {
                            let entry: ABIEntry = serde_json::from_value(item.clone()).unwrap();
                            if entry.entry_type == "function" {
                                if let (Some(name), Some(inputs)) = (entry.name, entry.inputs) {
                                    let selector = compute_selector(&name, &inputs);
                                    selectors.entry(selector).or_insert(name);
                                }
                            }
                        }
                    }
                } else {
                    eprintln!("Error parsing file: {:?} - ignoring.", file_path)
                }
            }
            Err(e) => eprintln!("Error reading file: {:?}", e),
        }
    }
    println!(
        "Analyzed {} files. Added {} selectors (before: {} after: {})",
        analyzed_files,
        selectors.len() - selectors_before,
        selectors_before,
        selectors.len()
    );

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output_file)?;
    serde_json::to_writer_pretty(file, &selectors)?;
    Ok(())
}

fn main() -> io::Result<()> {
    let args = Cli::parse();
    process_files(&args.contracts_dir, &args.output_file)
}
