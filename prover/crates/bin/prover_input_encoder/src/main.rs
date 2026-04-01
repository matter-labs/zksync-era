use clap::Parser;
use std::path::Path;
use zksync_airbender_prover_interface::inputs::AirbenderVerifierInput;

const DEFAULT_INPUT_FILE: &str = "proof_input_local.json";
const DEFAULT_OUTPUT_FILE: &str = "encoded_input.txt";

#[derive(Parser, Debug)]
#[command(version, about = "Encodes airbender verifier input into packed words")]
struct Cli {
    #[arg(short = 'i', long = "input", default_value = DEFAULT_INPUT_FILE)]
    input: String,

    #[arg(short = 'o', long = "output", default_value = DEFAULT_OUTPUT_FILE)]
    output: String,

    #[arg(long = "folder")]
    folder: bool,
}

fn encode_to_words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if bytes.len() > u32::MAX as usize {
        return Err("Encoded input is larger than u32::MAX bytes".to_string());
    }

    println!("Encoding {} bytes into words", bytes.len());

    let mut words = Vec::with_capacity(1 + (bytes.len() + 3) / 4);
    words.push(bytes.len() as u32);

    for chunk in bytes.chunks(4) {
        let mut buf = [0u8; 4];
        buf[..chunk.len()].copy_from_slice(chunk);
        words.push(u32::from_be_bytes(buf));
    }

    Ok(words)
}

#[cfg(test)]
fn decode_from_words(words: &[u32]) -> Result<Vec<u8>, String> {
    if words.is_empty() {
        return Err("No words provided".to_string());
    }

    let byte_len = words[0] as usize;
    let available = words.len().saturating_sub(1) * 4;
    if byte_len > available {
        return Err(format!(
            "Declared length {} exceeds available bytes {}",
            byte_len, available
        ));
    }

    let mut bytes = Vec::with_capacity(byte_len);
    for word in &words[1..] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes.truncate(byte_len);
    Ok(bytes)
}

fn encode_single_file(input_file: &Path, output_file: &Path) -> Result<(), String> {
    let json_input = std::fs::read_to_string(input_file)
        .map_err(|err| format!("Failed to read input file {}: {err}", input_file.display()))?;
    let verifier_input: AirbenderVerifierInput = serde_json::from_str(&json_input)
        .map_err(|err| format!("Failed to parse JSON input {}: {err}", input_file.display()))?;

    let encoded_input = bincode::serialize(&verifier_input).map_err(|err| {
        format!(
            "Failed to serialize verifier input {}: {err}",
            input_file.display()
        )
    })?;

    let words = encode_to_words(&encoded_input)?;
    let output = std::fs::File::create(output_file).map_err(|err| {
        format!(
            "Failed to create output file {}: {err}",
            output_file.display()
        )
    })?;
    let mut writer = std::io::BufWriter::new(output);
    for word in words {
        use std::io::Write;
        write!(writer, "{:08x}", word).map_err(|err| {
            format!(
                "Failed to write output word to {}: {err}",
                output_file.display()
            )
        })?;
    }

    Ok(())
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
            let json_input = std::fs::read_to_string(&input_path)
                .unwrap_or_else(|err| {
                    panic!("Failed to read input file {}: {err}", input_path.display())
                });
            let verifier_input: AirbenderVerifierInput = serde_json::from_str(&json_input)
                .unwrap_or_else(|err| {
                    panic!("Failed to parse JSON input {}: {err}", input_path.display())
                });

            let encoded_input = bincode::serialize(&verifier_input).unwrap_or_else(|err| {
                panic!(
                    "Failed to serialize verifier input {}: {err}",
                    input_path.display()
                )
            });

            let words = encode_to_words(&encoded_input).unwrap();
            let output = std::fs::File::create(&output_path).unwrap_or_else(|err| {
                panic!(
                    "Failed to create output file {}: {err}",
                    output_path.display()
                )
            });
            let mut writer = std::io::BufWriter::new(output);
            for word in words {
                use std::io::Write;
                write!(writer, "{:08x}", word).unwrap_or_else(|err| {
                    panic!(
                        "Failed to write output word to {}: {err}",
                        output_path.display()
                    )
                });
            }
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
    } else {
        encode_single_file(Path::new(&cli.input), Path::new(&cli.output))
    };

    if let Err(err) = result {
        panic!("{err}");
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_from_words, encode_to_words};

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
