use std::path::PathBuf;

use ethers::contract::Abigen;

fn main() -> eyre::Result<()> {
    let outdir = PathBuf::from(std::env::var("OUT_DIR")?).canonicalize()?;
    Abigen::new("ConsensusRegistry", "abi/ConsensusRegistry.json")?
        .generate()?
        .write_to_file(outdir.join("consensus_registry_abi.rs"))?;

    zksync_protobuf_build::Config {
        input_root: "src/commands/consensus/proto".into(),
        proto_root: "zksync/toolbox/consensus".into(),
        dependencies: vec!["::zksync_protobuf_config::proto".parse().unwrap()],
        protobuf_crate: "::zksync_protobuf".parse().unwrap(),
        is_public: false,
    }
    .generate()
    .unwrap();
    Ok(())
}
