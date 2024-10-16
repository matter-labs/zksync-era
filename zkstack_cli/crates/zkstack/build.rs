use std::path::PathBuf;

use dirs::{config_local_dir, home_dir};
use ethers::contract::Abigen;
use eyre::ContextCompat;

fn main() -> eyre::Result<()> {
    let outdir = PathBuf::from(std::env::var("OUT_DIR")?).canonicalize()?;
    Abigen::new("ConsensusRegistry", "abi/ConsensusRegistry.json")?
        .generate()?
        .write_to_file(outdir.join("consensus_registry_abi.rs"))?;

    // Copy completion scripts (ignore errors)
    copy_completion_scripts().ok();

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

fn copy_completion_scripts() -> eyre::Result<()> {
    let crate_name = env!("CARGO_PKG_NAME");

    // Create local config directory
    let local_config_dir = config_local_dir().unwrap().join(crate_name);
    std::fs::create_dir_all(&local_config_dir)?;

    // Array of supported shells
    let shells = ["bash", "zsh"];

    // Copy completion files
    let completion_dir = local_config_dir.join("completion");
    std::fs::create_dir_all(&completion_dir)?;

    for shell in &shells {
        let completion_file = format!("_{}_{}", crate_name, shell);
        std::fs::copy(
            format!("completion/{}", &completion_file),
            completion_dir.join(&completion_file),
        )?;

        // Source the completion file inside .{shell}rc
        let shell_rc = home_dir()
            .context("missing home directory")?
            .join(format!(".{}rc", shell));

        if shell_rc.exists() {
            let shell_rc_content = std::fs::read_to_string(&shell_rc)?;

            if !shell_rc_content.contains("# zkstack completion") {
                let completion_path = completion_dir.join(&completion_file);
                let completion_path = completion_path.to_str().unwrap();

                std::fs::write(
                    shell_rc,
                    format!(
                        "{}\n# zkstack completion\nsource \"{}\"\n",
                        shell_rc_content, completion_path
                    ),
                )?;
            }
        }
    }

    Ok(())
}
