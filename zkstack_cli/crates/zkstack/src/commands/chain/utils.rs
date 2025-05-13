use std::path::PathBuf;
use anyhow::Context;

use ethers::{abi::encode, types::{TransactionReceipt, TransactionRequest}, middleware::SignerMiddleware,     signers::{LocalWallet, Signer},
    utils::hex, providers::{Http, Middleware, Provider}};
use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;
use zksync_types::{web3::keccak256, Address, H256, L2_NATIVE_TOKEN_VAULT_ADDRESS, U256};

use crate::{
    admin_functions::AdminScriptOutput, commands::chain::admin_call_builder::AdminCallBuilder,
};

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = encode(&[
        ethers::abi::Token::Uint(l1_chain_id),
        ethers::abi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethers::abi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}

pub fn get_default_foundry_path(shell: &Shell) -> anyhow::Result<PathBuf> {
    Ok(EcosystemConfig::from_file(shell)?.path_to_l1_foundry())
}

pub fn display_admin_script_output(result: AdminScriptOutput) {
    let builder = AdminCallBuilder::new(result.calls);
    logger::info(format!(
        "Breakdown of calls to be performed by the chain admin:\n{}",
        builder.to_json_string()
    ));

    logger::info("\nThe calldata to be sent by the admin owner:".to_string());
    logger::info(format!("Admin address (to): {:#?}", result.admin_address));

    let (data, value) = builder.compile_full_calldata();

    logger::info(format!("Total data: {}", hex::encode(&data)));
    logger::info(format!("Total value: {}", value));
}

pub(crate) async fn send_tx(
    to: Address,
    data: Vec<u8>,
    value: U256,
    l1_rpc_url: String,
    private_key: H256,
    description: &str,
) -> anyhow::Result<TransactionReceipt> {
    // 1. Connect to provider
    let provider = Provider::<Http>::try_from(&l1_rpc_url)?;

    // 2. Set up wallet (signer)
    let wallet: LocalWallet = LocalWallet::from_bytes(private_key.as_bytes())?;
    let wallet = wallet.with_chain_id(provider.get_chainid().await?.as_u64()); // Mainnet

    // 3. Create a transaction
    let tx = TransactionRequest::new().to(to).data(data).value(value);

    let spinner = Spinner::new(&format!("Sending transaction for {description}..."));

    // 4. Sign the transaction
    let client = SignerMiddleware::new(provider.clone(), wallet.clone());
    let pending_tx = client.send_transaction(tx, None).await?;
    spinner.finish();

    logger::info(format!(
        "Transaction sent! Hash: {:#?}",
        pending_tx.tx_hash()
    ));

    let spinner = Spinner::new("Waiting for transaction to complete");

    // 5. Await receipt
    let receipt: TransactionReceipt = pending_tx.await?.context("Receipt not found")?;

    spinner.finish();

    logger::info(format!(
        "Transaciton {:#?} completed!",
        receipt.transaction_hash
    ));

    Ok(receipt)
}
