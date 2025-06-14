// src/wallets.rs
//! Derivation utilities plus *sequential* prefund logic.

use anyhow::Result;
use ethers::{
    prelude::*,
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder},
    types::{TransactionRequest, U256},
    utils::format_units,
};
use std::time::Duration;

/// Derive `n` wallets from the given mnemonic (BIP-44 path m/44'/60'/0'/0/i).
pub fn derive(mnemonic: &str, n: u32, chain_id: u64) -> Result<Vec<LocalWallet>> {
    let builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
    let mut wallets = Vec::with_capacity(n as usize);
    for i in 0..n {
        wallets.push(builder.clone().index(i)?.build()?.with_chain_id(chain_id));
    }
    Ok(wallets)
}

/// Fund every destination wallet **sequentially**.
///
/// * If a wallet already has at least `amount`, it is skipped.
/// * Otherwise we send a 21 000-gas transfer, wait for the receipt, then
///   poll the balance until the top-up is visible.
pub async fn prefund<S: Signer + 'static>(
    rich: &SignerMiddleware<Provider<Http>, S>,
    dests: &[Address],
    amount_wei: &str,
) -> Result<()> {
    let wei: U256 = amount_wei.parse()?;
    let provider  = rich.provider();

    for (i, dest) in dests.iter().enumerate() {
        let bal_before = provider.get_balance(*dest, None).await?;
        let bal_eth_before = format_units(bal_before, 18)?;
        let need_topup = bal_before < wei;

        println!(
            "Wallet #{:<3} {}  balance before: {} ETH  {}",
            i,
            dest,
            bal_eth_before.trim_end_matches('0').trim_end_matches('.'),
            if need_topup { "→ funding…" } else { "✓ already funded" }
        );

        if !need_topup {
            continue;
        }

        // send funding tx
        let pending = rich
            .send_transaction(
                TransactionRequest::pay(*dest, wei)
                    .from(rich.signer().address())
                    .gas(U256::from(21_000u64)),
                None,
            )
            .await?;

        println!("   tx hash: 0x{:x}  awaiting inclusion…", pending.tx_hash());
        pending.await?; // wait for mined receipt

        // poll until balance reflects the transfer
        loop {
            let bal_now = provider.get_balance(*dest, None).await?;
            if bal_now >= wei {
                let bal_eth_after = format_units(bal_now, 18)?;
                println!(
                    "   balance after:  {} ETH  ✅",
                    bal_eth_after.trim_end_matches('0').trim_end_matches('.')
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    Ok(())
}
