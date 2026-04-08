use std::io::{self, Read as _, Write as _};

use clap::Parser;
use zksync_airbender_prover_interface::{
    encoding::{decode_from_words, encode_input_to_hex},
    inputs::AirbenderVerifierInput,
};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Encodes/decodes airbender verifier input via stdin/stdout"
)]
struct Cli {
    /// Decode hex input back to JSON instead of encoding
    #[arg(short = 'd', long = "decode")]
    decode: bool,
}

fn encode() -> anyhow::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;

    let verifier_input: AirbenderVerifierInput = serde_json::from_str(&buf)?;
    let hex = encode_input_to_hex(&verifier_input)?;

    io::stdout().write_all(hex.as_bytes())?;
    Ok(())
}

fn decode() -> anyhow::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let hex_input = buf.trim();

    anyhow::ensure!(
        hex_input.len() % 8 == 0,
        "Hex input length {} is not a multiple of 8",
        hex_input.len()
    );

    let words: Vec<u32> = hex_input
        .as_bytes()
        .chunks(8)
        .enumerate()
        .map(|(i, chunk)| {
            let s = std::str::from_utf8(chunk)
                .map_err(|err| anyhow::anyhow!("Invalid UTF-8 at word {i}: {err}"))?;
            u32::from_str_radix(s, 16)
                .map_err(|err| anyhow::anyhow!("Invalid hex at word {i} ({s}): {err}"))
        })
        .collect::<Result<_, _>>()?;

    let bytes = decode_from_words(&words)?;
    let verifier_input: AirbenderVerifierInput = bincode::deserialize(&bytes)?;

    let json = serde_json::to_string_pretty(&verifier_input)?;
    io::stdout().write_all(json.as_bytes())?;
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = if cli.decode { decode() } else { encode() };

    if let Err(err) = result {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
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
