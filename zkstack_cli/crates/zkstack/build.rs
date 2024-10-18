use std::path::PathBuf;

use anyhow::{anyhow, Context};
use dirs::{config_local_dir, home_dir};
use ethers::contract::Abigen;

fn main() -> anyhow::Result<()> {
    let outdir = PathBuf::from(std::env::var("OUT_DIR")?).canonicalize()?;
    Abigen::new("ConsensusRegistry", "abi/ConsensusRegistry.json")
        .map_err(|_| anyhow!("Failed ABI deserialization"))?
        .generate()
        .map_err(|_| anyhow!("Failed ABI generation"))?
        .write_to_file(outdir.join("consensus_registry_abi.rs"))
        .context("Failed to write ABI to file")?;

    if let Err(e) = configure_shell_autocompletion() {
        println!("WARNING: It was not possible to install autocomplete scripts. Please generate them manually with `zkstack autocomplete`");
        println!("ERROR: {}", e);
    };

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

fn configure_shell_autocompletion() -> anyhow::Result<()> {
    let crate_name = env!("CARGO_PKG_NAME");

    // Create local config directory
    let local_config_dir = config_local_dir().unwrap().join(crate_name);
    std::fs::create_dir_all(&local_config_dir)
        .context("it was impossible to create the configuration directory")?;

    // Array of supported shells
    let shells = ["bash", "zsh"];

    // Copy completion files
    let completion_dir = local_config_dir.join("completion");

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
            let shell_rc_content = std::fs::read_to_string(&shell_rc)
                .context(format!("could not read .{}rc", shell))?;

            if !shell_rc_content.contains("# zkstack completion") {
                let completion_path = completion_dir.join(&completion_file);
                let completion_path = completion_path.to_str().unwrap();

                std::fs::write(
                    shell_rc,
                    format!(
                        "{}\n# zkstack completion\nsource \"{}\"\n",
                        shell_rc_content, completion_path
                    ),
                )
                .context(format!("could not write .{}rc", shell))?;
            }
        }
    }

    Ok(())
}
