use std::{path::Path, str::FromStr};

use anyhow::Context;
use tempfile::NamedTempFile;
use xshell::{cmd, Shell};
use zkstack_cli_common::logger;
use zkstack_cli_config::{CommitmentKeys, EcosystemConfig, GenesisConfig};
use zksync_types::H256;

use crate::consts::PATH_TO_PROVER_COMMITMENT;

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let vk_verification_key_data = NamedTempFile::new()?;

    let execution_environment = ecosystem_config
        .link_to_code
        .join("execution_environment/app.bin");
    if !execution_environment.exists() {
        anyhow::bail!(
            "Execution environment binary not found at: {}",
            execution_environment.display()
        );
    }
    let output_path = ecosystem_config
        .link_to_code
        .join("contracts")
        .join("tools")
        .join("data");
    let command = cmd!(
        shell,
        "cargo run --manifest-path ./zksync_os_snark_prover/Cargo.toml --release generate-keys"
    )
    .args([
        "--binary-path",
        execution_environment.to_str().unwrap(),
        "--output-dir",
        output_path.to_str().unwrap(),
        "--vk-verification-key-file",
        vk_verification_key_data.path().to_str().unwrap(),
    ]);
    command.run()?;
    let vk_verification_key =
        H256::from_str(&std::fs::read_to_string(vk_verification_key_data.path())?)
            .context("Malformed Verification key")?;
    update_verification_key_in_files(shell, &ecosystem_config.link_to_code, vk_verification_key)
        .await?;
    shell.copy_file(
        output_path.join("snark_vk_expected.json"),
        output_path.join("plonk_scheduler_key.json"),
    )?;
    shell.remove_path(output_path.join("snark_vk_expected.json"))?;

    let _guard = shell.push_dir("contracts/tools");
    cmd!(
        shell,
        "cargo run --bin zksync_verifier_contract_generator --release --"
    )
    .args([
        "--plonk_input_path",
        output_path
            .join("plonk_scheduler_key.json")
            .to_str()
            .unwrap(),
        "--fflonk_input_path",
        output_path
            .join("fflonk_scheduler_key.json")
            .to_str()
            .unwrap(),
        "--plonk_output_path",
        ecosystem_config
            .link_to_code
            .join("contracts/l1-contracts/contracts/state-transition/verifiers/L1VerifierPlonk.sol")
            .to_str()
            .unwrap(),
        "--fflonk_output_path",
        ecosystem_config
            .link_to_code
            .join(
                "contracts/l1-contracts/contracts/state-transition/verifiers/L1VerifierFflonk.sol",
            )
            .to_str()
            .unwrap(),
    ])
    .run()?;

    logger::info("Contracts updated successfully.");

    Ok(())
}

async fn update_verification_key_in_files(
    shell: &Shell,
    link_to_code: &Path,
    verification_key: H256,
) -> anyhow::Result<()> {
    let genesis_config_path =
        EcosystemConfig::default_configs_path(link_to_code).join("genesis.yaml");
    let mut genesis_config = GenesisConfig::read(&shell, genesis_config_path)
        .await
        .context("Failed to read genesis config")?
        .patched();
    genesis_config.update_snark_verification_key(verification_key)?;
    genesis_config
        .save()
        .await
        .context("Failed to save genesis config")?;
    let commitment_keys = link_to_code.join(PATH_TO_PROVER_COMMITMENT);
    let mut keys = CommitmentKeys::from_file(commitment_keys.to_str().unwrap())
        .context("Failed to read commitment keys")?;
    keys.snark_wrapper = verification_key;
    keys.save_to_file(commitment_keys.to_str().unwrap())
        .context("Failed to save commitment keys")?;
    Ok(())
}
