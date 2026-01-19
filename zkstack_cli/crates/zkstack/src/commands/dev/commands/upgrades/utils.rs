use ethers::contract::BaseContract;
use lazy_static::lazy_static;
use zkstack_cli_common::logger;

use crate::abi::ICHAINADMINABI_ABI;

lazy_static! {
    static ref CHAIN_ADMIN_ABI: BaseContract = BaseContract::from(ICHAINADMINABI_ABI.clone());
}

pub(crate) fn print_error(err: anyhow::Error) {
    logger::error(format!(
        "Chain is not ready to finalize the upgrade due to the reason:\n{:#?}",
        err
    ));
    logger::info("Once the chain is ready, you can re-run this command to obtain the calls to finalize the upgrade");
    logger::info("If you want to display finalization params anyway, pass `--force-display-finalization-params=true`.");
}

pub(crate) fn set_upgrade_timestamp_calldata(
    packed_protocol_version: u64,
    timestamp: u64,
) -> Vec<u8> {
    CHAIN_ADMIN_ABI
        .encode("setUpgradeTimestamp", (packed_protocol_version, timestamp))
        .unwrap()
        .to_vec()
}
