// src/wallets.rs
//! Wallet derivation helpers and *parallel* prefunding.

use anyhow::Result;
use ethers::{
    prelude::*,
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder},
    types::{TransactionRequest, U256},
};
use futures_util::future::join_all; // 0.3 futures

/// Derive `n` wallets from a BIP-39 mnemonic.
pub fn derive(mnemonic: &str, n: u32, chain_id: u64) -> Result<Vec<LocalWallet>> {
    let builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
    let mut ws = Vec::with_capacity(n as usize);
    for i in 0..n {
        ws.push(builder.clone().index(i)?.build()?.with_chain_id(chain_id));
    }
    Ok(ws)
}

/// Prefund every address with `amount` wei — now in parallel.
pub async fn prefund<S: Signer + 'static>(
    rich: &SignerMiddleware<Provider<Http>, S>,
    dests: &[Address],
    amount: &str,
) -> Result<()> {
    let wei: U256 = amount.parse()?;

    // starting nonce
    let mut next_nonce = rich
        .get_transaction_count(
            rich.signer().address(),
            Some(BlockNumber::Pending.into()),
        )
        .await?;

    // broadcast all txs
    let mut pendings = Vec::with_capacity(dests.len());
    for d in dests {
        println!("Prefunding {d:?} with {wei}");
        let tx = TransactionRequest::pay(*d, wei)
            .from(rich.signer().address())
            .gas(U256::from(21_000u64))
            .nonce(next_nonce);
        next_nonce += U256::one();
        pendings.push(rich.send_transaction(tx, None).await?);
    }

    // await ALL receipts concurrently
    join_all(pendings.into_iter().map(|p| async move { let _ = p.await; })).await;
    Ok(())
}
