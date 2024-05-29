use zksync_types::ProtocolVersionId;

fn main() {
    print!("{}\n", ProtocolVersionId::current_prover_version());
}
