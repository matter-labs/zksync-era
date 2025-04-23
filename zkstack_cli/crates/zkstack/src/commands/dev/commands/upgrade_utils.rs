use ethers::{
    abi::{parse_abi, Address, Token},
    contract::BaseContract,
    utils::hex,
};
use serde::Serialize;
use zkstack_cli_common::logger;
use zksync_basic_types::{ethabi, web3::Bytes, U256};
use zksync_contracts::{chain_admin_contract, hyperchain_contract, DIAMOND_CUT};

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
