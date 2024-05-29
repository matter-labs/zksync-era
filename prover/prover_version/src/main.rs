use zksync_types::ProtocolVersionId;

fn main() {
    println!("{}", ProtocolVersionId::current_prover_version());
}
