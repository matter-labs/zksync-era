use std::{ops::Add, time::Duration};

use ethers::{
    middleware::MiddlewareBuilder,
    prelude::{Http, LocalWallet, Provider, Signer},
    providers::Middleware,
    types::{Address, TransactionRequest},
};

use crate::wallets::Wallet;

pub async fn distribute_eth(
    main_wallet: Wallet,
    addresses: Vec<Address>,
    l1_rpc: String,
    chain_id: u32,
    amount: u128,
) -> anyhow::Result<()> {
    let wallet = LocalWallet::from_bytes(main_wallet.private_key.unwrap().as_bytes())?
        .with_chain_id(chain_id);
    let client = Provider::<Http>::try_from(l1_rpc)?.with_signer(wallet);

    let mut pending_txs = vec![];
    let mut nonce = client.get_transaction_count(client.address(), None).await?;
    for address in addresses {
        let tx = TransactionRequest::new()
            .to(address)
            .value(amount)
            .nonce(nonce)
            .chain_id(chain_id);
        nonce = nonce.add(1);
        pending_txs.push(
            client
                .send_transaction(tx, None)
                .await?
                // It's safe to set such low number of confirmations and low interval for localhost
                .confirmations(1)
                .interval(Duration::from_millis(30)),
        );
    }

    futures::future::join_all(pending_txs).await;
    Ok(())
}
