// src/erc20.rs
//! Mintable ERC-20 helpers with parallel distribution & debug logs.

use anyhow::Result;
use ethers::{
    contract::abigen,
    prelude::*,
    types::{Address, U256},
    utils::format_units,
};
use futures_util::future::join_all;
use std::{sync::Arc, time::Duration};

abigen!(
    SimpleERC20,
    "./contracts/out/SimpleERC20.sol/SimpleERC20.json"
);

pub use simple_erc20::SimpleERC20;

// ───────────── deploy + mint ─────────────
pub async fn deploy_and_mint<S: Signer + 'static>(
    signer: Arc<SignerMiddleware<Provider<Http>, S>>,
    name: &str,
    symbol: &str,
    initial_supply: U256,
) -> Result<SimpleERC20<SignerMiddleware<Provider<Http>, S>>> {
    println!("▶ Deploying ERC-20 {name}/{symbol} …");
    let token = SimpleERC20::deploy(signer.clone(), (name.to_owned(), symbol.to_owned()))?
        .confirmations(0usize)
        .send()
        .await?;
    println!("   deployed at {}", token.address());

    println!(
        "Total supply before mint: {}",
        format_units(token.total_supply().call().await?, 18)?
    );

    // keep ContractCall alive
    let call_mint    = token.mint(signer.address(), initial_supply);
    let pending_mint = call_mint.send().await?;
    let mint_hash: H256 = pending_mint.tx_hash();          // ← copy first
    println!("   mint tx hash: 0x{mint_hash:x}");
    pending_mint.await?.ok_or_else(|| anyhow::anyhow!("mint tx dropped"))?;

    println!(
        "Total supply after  mint: {}",
        format_units(token.total_supply().call().await?, 18)?
    );
    println!(
        "Deployer balance: {}",
        format_units(token.balance_of(signer.address()).call().await?, 18)?
    );
    println!();
    Ok(token)
}

// ───────────── parallel distribution ─────────────
pub async fn distribute<M: Middleware + 'static>(
    token: &SimpleERC20<M>,
    dests: &[Address],
    amount: U256,
) -> Result<()> {
    println!(
        "▶ Distributing {} tokens to {} wallets …",
        format_units(amount, 18)?,
        dests.len()
    );

    let provider = token.client().clone();
    let mut handles = Vec::with_capacity(dests.len());

    for (idx, &addr) in dests.iter().enumerate() {
        let call_transfer = token.transfer(addr, amount);
        let pending_tx    = call_transfer.send().await?;
        let tx_hash: H256 = pending_tx.tx_hash();          // copy

        // retry a short time to fetch nonce
        let mut nonce_repr = "?".to_string();
        for _ in 0..5 {
            if let Some(tx) = provider.get_transaction(tx_hash).await? {
                nonce_repr = tx.nonce.to_string();
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        println!(
            "   tx #{idx:<3} → {addr:?}  nonce {nonce_repr}  hash 0x{tx_hash:x}"
        );

        // spawn receipt poll
        let prov = provider.clone();
        handles.push(tokio::spawn(async move {
            loop {
                match prov.get_transaction_receipt(tx_hash).await {
                    Ok(Some(rcpt)) => {
                        if rcpt.status != Some(U64::one()) {
                            eprintln!(
                                "⚠️  transfer #{idx} to {addr:?} failed (status {:?})",
                                rcpt.status
                            );
                        }
                        break;
                    }
                    Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
                    Err(e)   => { eprintln!("⚠️  transfer #{idx} to {addr:?} error {e}"); break; }
                }
            }
        }));
    }

    join_all(handles).await;

    // summary
    println!("\n--- ERC-20 balances after distribution ---");
    for (i, addr) in dests.iter().enumerate() {
        let bal = token.balance_of(*addr).call().await?;
        println!("wallet {:>2}: {}", i, format_units(bal, 18)?);
    }
    println!("------------------------------------------------------------\n");
    Ok(())
}
