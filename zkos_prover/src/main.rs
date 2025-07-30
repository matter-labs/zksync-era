use clap::Parser;

#[tokio::main]
pub async fn main() {
    let args = zkos_prover::Args::parse();
    zkos_prover::run(args).await;
}
