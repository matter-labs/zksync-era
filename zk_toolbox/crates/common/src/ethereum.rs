use std::{ops::Add, time::Duration};

use ethers::prelude::Signer;
use ethers::{
    core::k256::ecdsa::SigningKey,
    middleware::MiddlewareBuilder,
    prelude::SignerMiddleware,
    prelude::{Http, LocalWallet, Provider},
    providers::Middleware,
    types::{Address as EthersAddress, TransactionRequest},
};

use crate::wallets::Wallet;
use alloy_primitives::{Address, B256};

pub fn create_ethers_client(
    private_key: B256,
    l1_rpc: String,
    chain_id: Option<u32>,
) -> anyhow::Result<SignerMiddleware<Provider<Http>, ethers::prelude::Wallet<SigningKey>>> {
    let mut wallet = LocalWallet::from_bytes(private_key.as_slice())?;
    if let Some(chain_id) = chain_id {
        wallet = wallet.with_chain_id(chain_id);
    }
    let client = Provider::<Http>::try_from(l1_rpc)?.with_signer(wallet);
    Ok(client)
}

pub async fn distribute_eth(
    main_wallet: Wallet,
    addresses: Vec<Address>,
    l1_rpc: String,
    chain_id: u32,
    amount: u128,
) -> anyhow::Result<()> {
    let client = create_ethers_client(main_wallet.private_key.unwrap(), l1_rpc, Some(chain_id))?;
    let mut pending_txs = vec![];
    let mut nonce = client.get_transaction_count(client.address(), None).await?;
    for address in addresses {
        let tx = TransactionRequest::new()
            .to(EthersAddress::from_slice(address.as_slice()))
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
