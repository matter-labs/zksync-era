use std::path::PathBuf;

use ethers::contract::Abigen;

fn main() -> eyre::Result<()> {
    let outdir = PathBuf::from(std::env::var("OUT_DIR")?).canonicalize()?;
    Abigen::new("ConsensusRegistry", "abi/ConsensusRegistry.json")?
        .generate()?
        .write_to_file(outdir.join("consensus_registry_abi.rs"))?;
    Ok(())
}
