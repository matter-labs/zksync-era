use ethers::{
    abi::{parse_abi, Address, Token},
    contract::BaseContract,
    utils::hex,
};
use serde::Serialize;
use zksync_contracts::{chain_admin_contract, hyperchain_contract, DIAMOND_CUT};
use zksync_types::{ethabi, web3::Bytes, U256};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminCall {
    pub(crate) description: String,
    pub(crate) target: Address,
    #[serde(serialize_with = "serialize_hex")]
    pub(crate) data: Vec<u8>,
    pub(crate) value: U256,
}

impl AdminCall {
    fn into_token(self) -> Token {
        let Self {
            target,
            data,
            value,
            ..
        } = self;
        Token::Tuple(vec![
            Token::Address(target),
            Token::Uint(value),
            Token::Bytes(data),
        ])
    }
}

fn serialize_hex<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let hex_string = format!("0x{}", hex::encode(bytes));
    serializer.serialize_str(&hex_string)
}

pub(crate) fn print_error(err: anyhow::Error) {
    println!(
        "Chain is not ready to finalize the upgrade due to the reason:\n{:#?}",
        err
    );
    println!("Once the chain is ready, you can re-run this command to obtain the calls to finalize the upgrade");
    println!("If you want to display finalization params anyway, pass `--force-display-finalization-params=true`.");
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
