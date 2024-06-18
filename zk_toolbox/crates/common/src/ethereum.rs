use std::{ops::Add, time::Duration};

use alloy::{
    network::EthereumWallet,
    primitives::{Address, B256, U256},
    providers::{
        fillers::{ChainIdFiller, GasFiller, NonceFiller},
        Provider, ProviderBuilder,
    },
    signers::local::PrivateKeySigner,
    transports::http::{Client, Http},
};
use url::Url;

use crate::wallets::Wallet;

pub fn create_ethers_client(
    private_key: B256,
    l1_rpc: Url,
    chain_id: Option<u64>,
) -> anyhow::Result<Box<dyn Provider<Http<Client>>>> {
    let wallet = EthereumWallet::from(PrivateKeySigner::from_bytes(&private_key)?);
    dbg!(&wallet);
    let client = ProviderBuilder::new()
        .filler(GasFiller)
        .filler(NonceFiller::default())
        .filler(ChainIdFiller::new(chain_id))
        .wallet(wallet)
        .on_http(l1_rpc);
    Ok(Box::new(client))
}

pub async fn distribute_eth(
    main_wallet: Wallet,
    addresses: Vec<Address>,
    l1_rpc: Url,
    chain_id: u64,
    amount: U256,
) -> anyhow::Result<()> {
    let client = create_ethers_client(main_wallet.private_key.unwrap(), l1_rpc, Some(chain_id))?;
    let mut pending_txs = vec![];

    let mut nonce = client
        .get_transaction_count(main_wallet.address)
        .await?
        .add(1);

    for address in addresses {
        let tx = client
            .transaction_request()
            .to(address)
            .value(amount)
            .nonce(nonce);
        pending_txs.push(
            client
                .send_transaction(tx)
                .await?
                // It's safe to set such low number of confirmations and low interval for localhost
                .with_required_confirmations(1)
                .with_timeout(Some(Duration::from_secs(10)))
                .watch(),
        );
        nonce = nonce.add(1);
    }

    futures::future::join_all(pending_txs).await;

    Ok(())
}

pub fn get_address_from_signer(signer: &PrivateKeySigner) -> Address {
    EthereumWallet::from(signer.clone())
        .default_signer()
        .address()
}

pub fn get_address_from_private_key(private_key: &B256) -> Address {
    let signer = PrivateKeySigner::from_bytes(private_key).unwrap();
    get_address_from_signer(&signer)
}
