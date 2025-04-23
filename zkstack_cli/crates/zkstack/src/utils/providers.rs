use std::{num::NonZeroUsize, str::FromStr, sync::Arc};

use anyhow::Context;
use ethers::providers::{Http, Provider};
use zksync_basic_types::{url::SensitiveUrl, L2ChainId};
use zksync_web3_decl::client::{Client, L2};

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
