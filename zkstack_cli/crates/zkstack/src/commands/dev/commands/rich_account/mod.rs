use args::RichAccountArgs;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::ZkStackConfig;
use zksync_basic_types::H256;

use crate::commands::dev::messages::msg_rich_account_outro;
pub mod args;
use alloy::{
    primitives::U256,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};

sol! {
    #[sol(rpc)]
    contract BridgehubAbi {
        struct L2TransactionRequestDirect {
            uint256 chainId;
            uint256 mintValue;
            address l2Contract;
            uint256 l2Value;
            bytes l2Calldata;
            uint256 l2GasLimit;
            uint256 l2GasPerPubdataByteLimit;
            bytes[] factoryDeps;
            address refundRecipient;
        }

        function requestL2TransactionDirect(L2TransactionRequestDirect _request) returns (bytes32 canonicalTxHash);
    }
}

pub async fn run(shell: &Shell, args: RichAccountArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    let chain_config = ZkStackConfig::current_chain(shell)?;

    let contracts_config = chain_config.get_contracts_config()?;
    let bridgehub_address = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;

    let signer: PrivateKeySigner = args.l1_account_private_key.parse()?;

    // Instantiate a provider with the signer
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .on_http(reqwest::Url::parse(&args.l1_rpc_url).unwrap());

    let gas_price = provider.get_gas_price().await?;

    let bridgehub = BridgehubAbi::new(bridgehub_address.0.into(), provider.clone());

    let mut tmp_bytes = [0u8; 32];
    args.amount.to_little_endian(&mut tmp_bytes);
    let amount = U256::from_le_bytes(tmp_bytes);

    let request = BridgehubAbi::L2TransactionRequestDirect {
        chainId: chain_config.chain_id.as_u64().try_into().unwrap(),
        mintValue: amount,
        l2Contract: args.l2_account.0.into(),
        l2Value: 0.try_into().unwrap(),
        l2Calldata: Default::default(),
        l2GasLimit: 1_000_000.try_into().unwrap(),
        l2GasPerPubdataByteLimit: 800.try_into().unwrap(),
        factoryDeps: Default::default(),
        refundRecipient: args.l2_account.0.into(),
    };

    let tx = bridgehub
        .requestL2TransactionDirect(request)
        .value(amount)
        .gas_price(gas_price * 2)
        .send()
        .await?;
    let receipt = tx.get_receipt().await?;
    logger::info(
        format!(
            "Transaction hash: {:?} ",
            H256::from(receipt.transaction_hash.0)
        )
        .as_str(),
    );

    if receipt.status() {
        logger::outro(msg_rich_account_outro(&format!("{:?}", args.l2_account)));
    } else {
        anyhow::bail!("Transaction failed");
    }
    Ok(())
}
