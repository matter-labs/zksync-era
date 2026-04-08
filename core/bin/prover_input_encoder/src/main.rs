use std::io::{self, Read as _, Write as _};

use zksync_airbender_prover_interface::inputs::AirbenderVerifierInput;

fn main() {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .expect("Failed to read stdin");

    let verifier_input: AirbenderVerifierInput =
        serde_json::from_str(&buf).expect("Failed to parse JSON input");

    let json =
        serde_json::to_string_pretty(&verifier_input).expect("Failed to serialize to JSON");
    io::stdout()
        .write_all(json.as_bytes())
        .expect("Failed to write to stdout");
}
