use xshell::Shell;
use zkstack_cli_config::traits::ReadConfig;
use zksync_types::commitment::L2DACommitmentScheme;

use crate::commands::dev::commands::upgrades::{
    args::v30_chain::V30ChainUpgradeArgs,
    default_chain_upgrade::{AdditionalUpgradeParams, NewDAValidators, UpgradeInfo, get_full_chain_upgrade_params, run_chain_upgrade},
};
use crate::commands::dev::commands::upgrades::types::UpgradeVersion;

const V30_UPGRADE_VERSION: UpgradeVersion = UpgradeVersion::V30ZkSyncOsBlobs;

pub(crate) async fn run(
    shell: &Shell,
    args_input: V30ChainUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    let upgrade_info = UpgradeInfo::read(
        shell,
        get_full_chain_upgrade_params(
            shell,
            args_input.base.clone(),
            V30_UPGRADE_VERSION
        ).await?.upgrade_description_path.unwrap()
    )?;

    run_chain_upgrade(
        shell,
        args_input.base.clone(),
        AdditionalUpgradeParams {
            updated_validators: None,
            updated_da_validators: Some(match args_input.da_mode {
                crate::commands::dev::commands::upgrades::args::v30_chain::SupportedPair::Validium => {
                    NewDAValidators {
                        new_l1_da_validator: upgrade_info.deployed_addresses.validium_l1_da_validator_addr.ok_or_else(|| {
                            anyhow::anyhow!("Validium L1 DA validator address is not set for this chain")
                        })?,
                        new_l2_commitment_scheme: L2DACommitmentScheme::EmptyNoDA,
                    }
                }
                crate::commands::dev::commands::upgrades::args::v30_chain::SupportedPair::RollupBlobs => {
                    NewDAValidators {
                        new_l1_da_validator: upgrade_info.deployed_addresses.blobs_zksync_os_l1_da_validator_addr.ok_or_else(|| {
                            anyhow::anyhow!("Blobs ZkSync OS L1 DA validator address is not set for this chain")
                        })?,
                        new_l2_commitment_scheme: L2DACommitmentScheme::BlobsZKSyncOS,
                    }
                }
                crate::commands::dev::commands::upgrades::args::v30_chain::SupportedPair::RollupCalldata => {
                    NewDAValidators {
                        new_l1_da_validator: upgrade_info.deployed_addresses.rollup_l1_da_validator_addr.ok_or_else(|| {
                            anyhow::anyhow!("Rollup Calldata L1 DA validator address is not set for this chain")
                        })?,
                        new_l2_commitment_scheme: L2DACommitmentScheme::BlobsAndPubdataKeccak256,
                    }
                }
            })
        },
        run_upgrade,
        V30_UPGRADE_VERSION,
    )
    .await
}
