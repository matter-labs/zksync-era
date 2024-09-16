use std::path::PathBuf;

use ethers::contract::Abigen;

fn main() -> eyre::Result<()> {
    let outdir = PathBuf::from(std::env::var("OUT_DIR")?).canonicalize()?;
    Abigen::new("ConsensusRegistry", "../../../contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json")?
        .generate()?
        .write_to_file(outdir.join("consensus_registry_abi.rs"))?;
    Ok(())
}
