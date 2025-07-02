use xshell::{cmd, Shell};
use zkstack_cli_common::logger;
use zkstack_cli_config::EcosystemConfig;

pub(crate) fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

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
    ]);
    command.run()?;
    logger::info(format!(
        "Snark keys generated successfully. Find them in: {}",
        output_path.display(),
    ));
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
