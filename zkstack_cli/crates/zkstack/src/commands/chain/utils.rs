use anyhow::Context;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{TransactionReceipt, TransactionRequest},
    utils::hex,
};
use zkstack_cli_common::{logger, spinner::Spinner};
use zksync_types::{Address, H256, U256};

use crate::{
    admin_functions::AdminScriptOutput, commands::chain::admin_call_builder::AdminCallBuilder,
};

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

    if let Some(status) = receipt.status {
        if status.as_u64() == 0 {
            anyhow::bail!(
                "Transaction {:#?} failed (reverted)!",
                receipt.transaction_hash
            );
        }
    }

    logger::info(format!(
        "Transaction {:#?} completed!",
        receipt.transaction_hash
    ));

    Ok(receipt)
}
