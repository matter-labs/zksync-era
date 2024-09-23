use std::{ops::Add, sync::Arc, time::Duration};

use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::MiddlewareBuilder,
    prelude::{Http, LocalWallet, Provider, Signer, SignerMiddleware},
    providers::Middleware,
    types::{Address, TransactionRequest, H256},
};
use types::TokenInfo;

use crate::wallets::Wallet;

pub fn create_ethers_client(
    private_key: H256,
    l1_rpc: String,
    chain_id: Option<u64>,
) -> anyhow::Result<SignerMiddleware<Provider<Http>, ethers::prelude::Wallet<SigningKey>>> {
    let mut wallet = LocalWallet::from_bytes(private_key.as_bytes())?;
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
    chain_id: u64,
    amount: u128,
) -> anyhow::Result<()> {
    let client = create_ethers_client(main_wallet.private_key.unwrap(), l1_rpc, Some(chain_id))?;
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

abigen!(
    TokenContract,
    r"[
    function name() external view returns (string)
    function symbol() external view returns (string)
    function decimals() external view returns (uint8)
    function mint(address to, uint256 amount)
    ]"
);

pub async fn get_token_info(token_address: Address, rpc_url: String) -> anyhow::Result<TokenInfo> {
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let contract = TokenContract::new(token_address, Arc::new(provider));

    let name = contract.name().call().await?;
    let symbol = contract.symbol().call().await?;
    let decimals = contract.decimals().call().await?;

    Ok(TokenInfo {
        name,
        symbol,
        decimals,
    })
}

pub async fn mint_token(
    main_wallet: Wallet,
    token_address: Address,
    addresses: Vec<Address>,
    l1_rpc: String,
    chain_id: u64,
    amount: u128,
) -> anyhow::Result<()> {
    let client = Arc::new(
        create_ethers_client(main_wallet.private_key.unwrap(), l1_rpc, Some(chain_id))?
            .nonce_manager(main_wallet.address),
    );

    let contract = TokenContract::new(token_address, client);

    let mut pending_calls = vec![];
    for address in addresses {
        pending_calls.push(contract.mint(address, amount.into()));
    }

    let mut pending_txs = vec![];
    for call in &pending_calls {
        pending_txs.push(
            call.send()
                .await?
                // It's safe to set such low number of confirmations and low interval for localhost
                .confirmations(3)
                .interval(Duration::from_millis(30)),
        );
    }

    futures::future::join_all(pending_txs).await;

    Ok(())
}
