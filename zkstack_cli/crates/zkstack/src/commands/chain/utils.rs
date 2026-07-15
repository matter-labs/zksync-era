use std::process::Command;

use anyhow::Context;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{
        transaction::eip2718::TypedTransaction, TransactionReceipt, TransactionRequest,
        U256 as EthersU256,
    },
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
    // Set `from` explicitly so estimateGas does not use an arbitrary zero-balance account.
    let mut tx = TransactionRequest::new()
        .from(wallet.address())
        .to(to)
        .data(data)
        .value(value);

    let gas_estimate = {
        let typed_tx: TypedTransaction = tx.clone().into();
        provider.estimate_gas(&typed_tx, None).await
    };
    let (gas_estimate_for_log, gas_limit) = match gas_estimate {
        Ok(estimate) => {
            let padded_estimate = (estimate * EthersU256::from(6_u64)) / EthersU256::from(5_u64);
            let fallback = EthersU256::from(1_500_000_u64);
            (Some(estimate), std::cmp::max(padded_estimate, fallback))
        }
        Err(err) => {
            let err_text = err.to_string().to_lowercase();
            if err_text.contains("execution reverted") || err_text.contains("revert") {
                return Err(err.into());
            }
            let fallback = EthersU256::from(1_500_000_u64);
            logger::warn(format!(
                "Failed to estimate gas for {description}: {err}. Falling back to gas limit {fallback}"
            ));
            (None, fallback)
        }
    };
    tx = tx.gas(gas_limit);
    if let Some(estimate) = gas_estimate_for_log {
        logger::info(format!(
            "Using gas limit {} (estimate {}) for {}",
            gas_limit, estimate, description
        ));
    }

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
            let tx_hash = format!("{:#?}", receipt.transaction_hash);
            logger::error(format!("Transaction {} failed (reverted)!", tx_hash));
            logger::info(format!(
                "Running `cast run {} --rpc-url {}` to get the execution trace...",
                tx_hash, l1_rpc_url
            ));
            match Command::new("cast")
                .args(["run", &tx_hash, "--rpc-url", &l1_rpc_url])
                .output()
            {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stdout.is_empty() {
                        println!("=== cast run trace ===\n{stdout}\n=== end trace ===");
                    }
                    if !stderr.is_empty() {
                        eprintln!("=== cast run stderr ===\n{stderr}\n=== end stderr ===");
                    }
                }
                Err(e) => {
                    logger::warn(format!(
                        "Failed to run `cast run` (is foundry installed?): {e}"
                    ));
                }
            }
            anyhow::bail!("Transaction {} failed (reverted)!", tx_hash);
        }
    }

    logger::info(format!(
        "Transaction {:#?} completed!",
        receipt.transaction_hash
    ));

    Ok(receipt)
}
