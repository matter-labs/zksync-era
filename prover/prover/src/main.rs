#![cfg_attr(not(feature = "gpu"), allow(unused_imports))]

#[cfg(feature = "gpu")]
mod artifact_provider;
#[cfg(feature = "gpu")]
mod prover;
#[cfg(feature = "gpu")]
mod prover_params;
#[cfg(feature = "gpu")]
mod run;
#[cfg(feature = "gpu")]
mod socket_listener;
#[cfg(feature = "gpu")]
mod synthesized_circuit_provider;

#[cfg(not(feature = "gpu"))]
fn main() {
    unimplemented!("This binary is only available with `gpu` feature enabled");
}

#[cfg(feature = "gpu")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run::run().await
}
