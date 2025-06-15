// src/erc20.rs
//! ERC-20 deploy, mint, and sequential variable distribution.

use anyhow::Result;
use ethers::{
    contract::abigen,
    prelude::*,
    types::{Address, U256},
    utils::format_units,
};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};

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
    supply: U256,
) -> Result<SimpleERC20<SignerMiddleware<Provider<Http>, S>>> {
    println!("▶ Deploying {name}/{symbol} …");
    let token = SimpleERC20::deploy(signer.clone(), (name.to_owned(), symbol.to_owned()))?
        .confirmations(0usize)
        .send()
        .await?;
    println!("   deployed at {}\n", token.address());

    let call_mint    = token.mint(signer.address(), supply);
    let pending_mint = call_mint.send().await?;
    println!("   mint tx hash 0x{:x}", pending_mint.tx_hash());
    pending_mint.await?;

    let supply_eth = format_units(token.total_supply().call().await?, 18)?;
    println!("   total supply {supply_eth}\n");
    Ok(token)
}

// ───────────── sequential distribution ─────────────
pub async fn distribute_varied<M: Middleware + 'static>(
    token: &SimpleERC20<M>,
    dests: &[Address],
    amounts: &[U256],
) -> Result<()> {
    assert_eq!(dests.len(), amounts.len(), "length mismatch");
    println!("▶ Distributing tokens sequentially …");

    let provider = token.client().clone();
    let timeout_s = 30;

    for (i, (&addr, &amt)) in dests.iter().zip(amounts).enumerate() {
        // 1. broadcast
        let call = token.transfer(addr, amt);
        let pending = call.send().await?;
        let tx_hash = pending.tx_hash();
        println!(
            "   tx #{:<3} → {addr:?}  amt {:>12}  hash 0x{tx_hash:x}",
            i,
            format_units(amt, 18)?
        );

        // 2. wait for receipt with timeout
        match timeout(Duration::from_secs(timeout_s), async {
            loop {
                match provider.get_transaction_receipt(tx_hash).await {
                    Ok(Some(rcpt)) => break Ok(rcpt),
                    Ok(None)       => sleep(Duration::from_millis(150)).await,
                    Err(e)         => break Err(anyhow::anyhow!(e)),
                }
            }
        })
            .await
        {
            Ok(Ok(rcpt)) if rcpt.status == Some(U64::one()) => {
                println!("      ✅ tx 0x{tx_hash:x} success (block {})",
                         rcpt.block_number.unwrap());
            }
            Ok(Ok(rcpt)) => {
                println!("      ⚠️ tx 0x{tx_hash:x} reverted (status {:?})",
                         rcpt.status);
            }
            Ok(Err(e)) => {
                println!("      ⚠️ tx 0x{tx_hash:x} error {e}");
            }
            Err(_) => {
                println!("      ⏳ tx 0x{tx_hash:x} timed-out after {timeout_s}s");
            }
        }
    }

    println!("   ✅ distribution phase finished\n");
    Ok(())
}
