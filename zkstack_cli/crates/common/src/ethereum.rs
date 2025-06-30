use std::{num::NonZeroUsize, ops::Add, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::MiddlewareBuilder,
    prelude::{Http, LocalWallet, Provider, Signer, SignerMiddleware},
    providers::Middleware,
    types::{Address, TransactionRequest},
};
use zkstack_cli_types::TokenInfo;
use zksync_types::{url::SensitiveUrl, L2ChainId};
use zksync_web3_decl::client::{Client, L2};

use crate::{logger, wallets::Wallet};

pub fn get_ethers_provider(url: &str) -> anyhow::Result<Arc<Provider<Http>>> {
    let provider = match Provider::<Http>::try_from(url) {
        Ok(provider) => provider,
        Err(err) => {
            anyhow::bail!("Connection error: {:#?}", err);
        }
    };

    Ok(Arc::new(provider))
}

pub fn get_zk_client(url: &str, l2_chain_id: u64) -> anyhow::Result<Client<L2>> {
    let client = Client::http(SensitiveUrl::from_str(url).unwrap())
        .context("failed creating JSON-RPC client for main node")?
        .for_network(L2ChainId::new(l2_chain_id).unwrap().into())
        .with_allowed_requests_per_second(NonZeroUsize::new(100_usize).unwrap())
        .build();

    Ok(client)
}

pub async fn get_zk_client_from_url(url: &str) -> anyhow::Result<Client<L2>> {
    // Creating a fully functional `zk client` requires having `l2_chain_id`.
    // We will create ethers provider to fetch the chain id and then create the zk client

    let chain_id = get_ethers_provider(url)?.get_chainid().await?.as_u64();
    get_zk_client(url, chain_id)
}

pub fn create_ethers_client(
    mut wallet: LocalWallet,
    l1_rpc: String,
    chain_id: Option<u64>,
) -> anyhow::Result<SignerMiddleware<Provider<Http>, ethers::prelude::Wallet<SigningKey>>> {
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
                .confirmations(5)
                .interval(Duration::from_millis(300)),
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
        let call = call.send().await;
        match call {
            // It's safe to set such low number of confirmations and low interval for localhost
            Ok(call) => pending_txs.push(call.confirmations(5).interval(Duration::from_millis(300))),
            Err(e) => logger::error(format!("Minting is not successful {e}")),
        }
    }

    futures::future::join_all(pending_txs).await;

    Ok(())
}
