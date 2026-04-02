use std::{
    io::{self, Read as _, Write as _},
    path::Path,
};

use clap::Parser;
use zksync_airbender_prover_interface::{
    encoding::{decode_from_words, encode_input_to_hex},
    inputs::AirbenderVerifierInput,
};

const DEFAULT_INPUT_FILE: &str = "proof_input_local.json";
const DEFAULT_OUTPUT_FILE: &str = "encoded_input.txt";

#[derive(Parser, Debug)]
#[command(version, about = "Encodes airbender verifier input into packed words")]
struct Cli {
    /// Input file path, or "-" to read from stdin
    #[arg(short = 'i', long = "input", default_value = DEFAULT_INPUT_FILE)]
    input: String,

    /// Output file path, or "-" to write to stdout
    #[arg(short = 'o', long = "output", default_value = DEFAULT_OUTPUT_FILE)]
    output: String,

    /// Process all JSON files in input folder, writing to output folder
    #[arg(long = "folder")]
    folder: bool,

    /// Decode hex input back to JSON instead of encoding
    #[arg(short = 'd', long = "decode")]
    decode: bool,
}

fn read_input(input: &str) -> Result<String, String> {
    if input == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|err| format!("Failed to read stdin: {err}"))?;
        Ok(buf)
    } else {
        std::fs::read_to_string(input)
            .map_err(|err| format!("Failed to read input file {input}: {err}"))
    }
}

fn write_output(output: &str, data: &str) -> Result<(), String> {
    if output == "-" {
        io::stdout()
            .write_all(data.as_bytes())
            .map_err(|err| format!("Failed to write to stdout: {err}"))
    } else {
        std::fs::write(output, data)
            .map_err(|err| format!("Failed to write output file {output}: {err}"))
    }
}

fn encode_single(input: &str, output: &str) -> Result<(), String> {
    let json_input = read_input(input)?;
    let verifier_input: AirbenderVerifierInput = serde_json::from_str(&json_input)
        .map_err(|err| format!("Failed to parse JSON input: {err}"))?;

    let hex = encode_input_to_hex(&verifier_input)?;
    write_output(output, &hex)
}

fn decode_single(input: &str, output: &str) -> Result<(), String> {
    let hex_input = read_input(input)?;
    let hex_input = hex_input.trim();

    if hex_input.len() % 8 != 0 {
        return Err(format!(
            "Hex input length {} is not a multiple of 8",
            hex_input.len()
        ));
    }

    let words: Vec<u32> = hex_input
        .as_bytes()
        .chunks(8)
        .enumerate()
        .map(|(i, chunk)| {
            let s = std::str::from_utf8(chunk)
                .map_err(|err| format!("Invalid UTF-8 at word {i}: {err}"))?;
            u32::from_str_radix(s, 16)
                .map_err(|err| format!("Invalid hex at word {i} ({s}): {err}"))
        })
        .collect::<Result<_, _>>()?;

    let bytes = decode_from_words(&words)?;
    let verifier_input: AirbenderVerifierInput = bincode::deserialize(&bytes)
        .map_err(|err| format!("Failed to deserialize verifier input: {err}"))?;

    let json = serde_json::to_string_pretty(&verifier_input)
        .map_err(|err| format!("Failed to serialize to JSON: {err}"))?;
    write_output(output, &json)
}

fn encode_missing_from_folders(input_folder: &Path, output_folder: &Path) -> Result<(), String> {
    if !input_folder.exists() || !input_folder.is_dir() {
        return Err(format!(
            "Input path must be an existing folder: {}",
            input_folder.display()
        ));
    }
    if !output_folder.exists() || !output_folder.is_dir() {
        return Err(format!(
            "Output path must be an existing folder: {}",
            output_folder.display()
        ));
    }

    let entries = std::fs::read_dir(input_folder).map_err(|err| {
        format!(
            "Failed to read input folder {}: {err}",
            input_folder.display()
        )
    })?;

    let mut handles = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|err| format!("Failed to read folder entry: {err}"))?;
        let input_path = entry.path();
        if !input_path.is_file() {
            continue;
        }

        let is_json = input_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
        if !is_json {
            continue;
        }

        let file_stem = input_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| {
                format!(
                    "Input file name is not valid UTF-8: {}",
                    input_path.display()
                )
            })?
            .to_string();

        let output_path = output_folder.join(format!("{file_stem}.bin"));
        if output_path.exists() {
            continue;
        }

        let handle = std::thread::spawn(move || {
            let json_input = std::fs::read_to_string(&input_path).unwrap_or_else(|err| {
                panic!("Failed to read input file {}: {err}", input_path.display())
            });
            let verifier_input: AirbenderVerifierInput = serde_json::from_str(&json_input)
                .unwrap_or_else(|err| {
                    panic!("Failed to parse JSON input {}: {err}", input_path.display())
                });

            let hex = encode_input_to_hex(&verifier_input).unwrap();

            std::fs::write(&output_path, hex).unwrap_or_else(|err| {
                panic!(
                    "Failed to write output file {}: {err}",
                    output_path.display()
                )
            });
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = if cli.folder {
        encode_missing_from_folders(Path::new(&cli.input), Path::new(&cli.output))
    } else if cli.decode {
        decode_single(&cli.input, &cli.output)
    } else {
        encode_single(&cli.input, &cli.output)
    };

    if let Err(err) = result {
        panic!("{err}");
    }
}

#[cfg(test)]
mod tests {
    use zksync_airbender_prover_interface::encoding::{decode_from_words, encode_to_words};

    #[test]
    fn encode_decode_roundtrip_with_padding() {
        let input = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let words = encode_to_words(&input).expect("encode");
        assert_eq!(words[0], 5);
        assert_eq!(words.len(), 3);
        assert_eq!(words[1], 0x01020304);
        assert_eq!(words[2], 0x05000000);

        let decoded = decode_from_words(&words).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn encode_decode_roundtrip_empty() {
        let input = Vec::<u8>::new();
        let words = encode_to_words(&input).expect("encode");
        assert_eq!(words, vec![0]);
        let decoded = decode_from_words(&words).expect("decode");
        assert!(decoded.is_empty());
    }
}
