use prover_cli::cli;

#[tokio::main]
async fn main() {
    cli::start().await.unwrap();
}
