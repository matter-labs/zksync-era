use anyhow::Context;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{BlockId, TransactionReceipt, TransactionRequest},
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
    let pending_tx = client.send_transaction(tx.clone(), None).await?;
    spinner.finish();

    logger::info(format!(
        "Transaction sent! Hash: {:#?}",
        pending_tx.tx_hash()
    ));

    let spinner = Spinner::new("Waiting for transaction to complete");

    // 5. Await receipt
    let receipt: TransactionReceipt = pending_tx.await?.context("Receipt not found")?;

    spinner.finish();

    // 6. CHECK STATUS AND GET REVERT REASON IF FAILED
    let tx_hash = receipt.transaction_hash;
    if receipt.status != Some(1.into()) {
        logger::error("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        logger::error("❌ TRANSACTION REVERTED");
        logger::error("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        logger::error(format!("Transaction: {:#?}", receipt.transaction_hash));
        logger::error(format!("Block: {:?}", receipt.block_number));
        logger::error(format!("Gas used: {:?}", receipt.gas_used));
        logger::error("");

        // Try to get the actual revert reason by replaying the transaction
        logger::error("Attempting to extract revert reason...");

        // Get the transaction to replay it
        if let Ok(Some(tx)) = provider.get_transaction(tx_hash).await {
            // Replay at the block before it was mined
            let replay_block = BlockId::Number((receipt.block_number.unwrap().as_u64() - 1).into());

            let replay_tx = TransactionRequest::new()
                .to(to)
                .from(tx.from)
                .data(tx.input.clone())
                .value(tx.value)
                .gas(tx.gas);

            match provider.call(&replay_tx.into(), Some(replay_block)).await {
                Ok(_) => {
                    logger::error(
                        "⚠ Replay succeeded - this was a transient state issue (race condition)",
                    );
                }
                Err(e) => {
                    logger::error("REVERT REASON:");
                    logger::error(format!("{:#?}", e));

                    // Try to extract human-readable error
                    let error_str = format!("{:?}", e);
                    if error_str.contains("execution reverted: ") {
                        if let Some(msg_start) = error_str.find("execution reverted: ") {
                            let msg = &error_str[msg_start + 20..];
                            if let Some(msg_end) = msg.find('"') {
                                logger::error("");
                                logger::error(&format!("📝 Error message: {}", &msg[..msg_end]));
                            }
                        }
                    }
                }
            }
        }

        // Also try re-simulation with current state
        logger::error("");
        logger::error("Re-simulating with current state...");
        match provider.call(&tx.into(), None).await {
            Ok(_) => {
                logger::error("✓ Current state is valid - this was a temporary issue");
                logger::error(
                    "  RECOMMENDATION: Retry the transaction or add delay before sending",
                );
            }
            Err(e) => {
                logger::error("✗ Current state still invalid:");
                logger::error(format!("  {:#?}", e));
            }
        }

        logger::error("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        anyhow::bail!(
            "Transaction {} reverted. See error details above.",
            receipt.transaction_hash
        );
    }

    logger::info(format!(
        "Transaction {:#?} completed!",
        receipt.transaction_hash
    ));

    Ok(receipt)
}
