use ethers::{abi::parse_abi, contract::BaseContract};
use zkstack_cli_common::logger;

pub(crate) fn print_error(err: anyhow::Error) {
    logger::error(format!(
        "Chain is not ready to finalize the upgrade due to the reason:\n{:#?}",
        err
    ));
    logger::info("Once the chain is ready, you can re-run this command to obtain the calls to finalize the upgrade");
    logger::info("If you want to display finalization params anyway, pass `--force-display-finalization-params=true`.");
}

fn chain_admin_abi() -> BaseContract {
    BaseContract::from(
        parse_abi(&[
            "function setUpgradeTimestamp(uint256 _protocolVersion, uint256 _upgradeTimestamp) external",
        ])
        .unwrap(),
    )
}

pub(crate) fn set_upgrade_timestamp_calldata(
    packed_protocol_version: u64,
    timestamp: u64,
) -> Vec<u8> {
    let chain_admin = chain_admin_abi();

    chain_admin
        .encode("setUpgradeTimestamp", (packed_protocol_version, timestamp))
        .unwrap()
        .to_vec()
}
