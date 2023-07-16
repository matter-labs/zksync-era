//! This file hold tools used for test-forking other networks.
//!
//! There is ForkStorage (that is a wrapper over InMemoryStorage)
//! And ForkDetails - that parses network address and fork height from arguments.

use std::{
    collections::HashMap,
    convert::TryInto,
    future::Future,
    sync::{Arc, RwLock},
};

use tokio::runtime::Builder;
use zksync_basic_types::{L1BatchNumber, L2ChainId, MiniblockNumber, H256, U64};

use zksync_state::{InMemoryStorage, ReadStorage};
use zksync_types::{
    api::{BlockIdVariant, BlockNumber},
    l2::L2Tx,
    StorageKey,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use zksync_web3_decl::{jsonrpsee::http_client::HttpClient, namespaces::EthNamespaceClient};
use zksync_web3_decl::{jsonrpsee::http_client::HttpClientBuilder, namespaces::ZksNamespaceClient};

use crate::node::TEST_NODE_NETWORK_ID;

fn block_on<F: Future + Send + 'static>(future: F) -> F::Output
where
    F::Output: Send,
{
    std::thread::spawn(move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime creation failed");
        runtime.block_on(future)
    })
    .join()
    .unwrap()
}

/// In memory storage, that allows 'forking' from other network.
/// If forking is enabled, it reads missing data from remote location.
#[derive(Debug)]
pub struct ForkStorage {
    pub inner: Arc<RwLock<ForkStorageInner>>,
    pub chain_id: L2ChainId,
}

#[derive(Debug)]
pub struct ForkStorageInner {
    // Underlying local storage
    pub raw_storage: InMemoryStorage,
    // Cache of data that was read from remote location.
    pub value_read_cache: HashMap<StorageKey, H256>,
    // Cache of factory deps that were read from remote location.
    pub factory_dep_cache: HashMap<H256, Option<Vec<u8>>>,
    // If set - it hold the necessary information on where to fetch the data.
    // If not set - it will simply read from underlying storage.
    pub fork: Option<ForkDetails>,
}

impl ForkStorage {
    pub fn new(fork: Option<ForkDetails>) -> Self {
        let chain_id = fork
            .as_ref()
            .and_then(|d| d.overwrite_chain_id)
            .unwrap_or(L2ChainId(TEST_NODE_NETWORK_ID));
        println!("Starting network with chain id: {:?}", chain_id);

        ForkStorage {
            inner: Arc::new(RwLock::new(ForkStorageInner {
                raw_storage: InMemoryStorage::with_system_contracts_and_chain_id(
                    chain_id,
                    hash_bytecode,
                ),
                value_read_cache: Default::default(),
                fork,
                factory_dep_cache: Default::default(),
            })),
            chain_id,
        }
    }

    fn read_value_internal(&self, key: &StorageKey) -> zksync_types::StorageValue {
        let mut mutator = self.inner.write().unwrap();
        let local_storage = mutator.raw_storage.read_value(key);

        if let Some(fork) = &mutator.fork {
            if !H256::is_zero(&local_storage) {
                return local_storage;
            }

            if let Some(value) = mutator.value_read_cache.get(key) {
                return *value;
            }
            let fork_ = (*fork).clone();
            let key_ = *key;

            let client = fork.create_client();

            let result = block_on(async move {
                client
                    .get_storage_at(
                        *key_.account().address(),
                        h256_to_u256(*key_.key()),
                        Some(BlockIdVariant::BlockNumber(BlockNumber::Number(U64::from(
                            fork_.l2_miniblock,
                        )))),
                    )
                    .await
            })
            .unwrap();

            mutator.value_read_cache.insert(*key, result);
            result
        } else {
            local_storage
        }
    }

    pub fn load_factory_dep_internal(&self, hash: H256) -> Option<Vec<u8>> {
        let mut mutator = self.inner.write().unwrap();
        let local_storage = mutator.raw_storage.load_factory_dep(hash);
        if let Some(fork) = &mutator.fork {
            if local_storage.is_some() {
                return local_storage;
            }
            if let Some(value) = mutator.factory_dep_cache.get(&hash) {
                return value.clone();
            }

            let client = fork.create_client();
            let result = block_on(async move { client.get_bytecode_by_hash(hash).await }).unwrap();
            mutator.factory_dep_cache.insert(hash, result.clone());
            result
        } else {
            local_storage
        }
    }
}

impl ReadStorage for ForkStorage {
    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        (&*self).is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        (&*self).load_factory_dep(hash)
    }

    fn read_value(&mut self, key: &StorageKey) -> zksync_types::StorageValue {
        (&*self).read_value(key)
    }
}

impl ReadStorage for &ForkStorage {
    fn read_value(&mut self, key: &StorageKey) -> zksync_types::StorageValue {
        self.read_value_internal(key)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let mut mutator = self.inner.write().unwrap();
        mutator.raw_storage.is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.load_factory_dep_internal(hash)
    }
}

impl ForkStorage {
    pub fn set_value(&mut self, key: StorageKey, value: zksync_types::StorageValue) {
        let mut mutator = self.inner.write().unwrap();
        mutator.raw_storage.set_value(key, value)
    }
    pub fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        let mut mutator = self.inner.write().unwrap();
        mutator.raw_storage.store_factory_dep(hash, bytecode)
    }
}

/// Holds the information about the original chain.
#[derive(Debug, Clone)]
pub struct ForkDetails {
    // URL to the server.
    pub fork_url: String,
    // Block number at which we forked (the next block to create is l1_block + 1)
    pub l1_block: L1BatchNumber,
    pub l2_miniblock: u64,
    pub block_timestamp: u64,
    pub overwrite_chain_id: Option<L2ChainId>,
}

impl ForkDetails {
    pub async fn from_url_and_miniblock_and_chain(
        url: &str,
        client: HttpClient,
        miniblock: u64,
        chain_id: Option<L2ChainId>,
    ) -> Self {
        let block_details = client
            .get_block_details(MiniblockNumber(miniblock as u32))
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Could not find block {:?} in {:?}", miniblock, url));

        let l1_batch_number = block_details.l1_batch_number;

        println!(
            "Creating fork from {:?} L1 block: {:?} L2 block: {:?} with timestamp {:?}",
            url, l1_batch_number, miniblock, block_details.timestamp
        );

        ForkDetails {
            fork_url: url.to_owned(),
            l1_block: l1_batch_number,
            block_timestamp: block_details.timestamp,
            l2_miniblock: miniblock,
            overwrite_chain_id: chain_id,
        }
    }

    /// Create a fork from a given network at a given height.
    pub async fn from_network(fork: &str, fork_at: Option<u64>) -> Self {
        let (url, client) = Self::fork_to_url_and_client(fork);
        let l2_miniblock = if let Some(fork_at) = fork_at {
            fork_at
        } else {
            client.get_block_number().await.unwrap().as_u64()
        };
        Self::from_url_and_miniblock_and_chain(url, client, l2_miniblock, None).await
    }

    /// Create a fork from a given network, at a height BEFORE a transaction.
    /// This will allow us to apply this transaction locally on top of this fork.
    pub async fn from_network_tx(fork: &str, tx: H256) -> Self {
        let (url, client) = Self::fork_to_url_and_client(fork);
        let tx_details = client.get_transaction_by_hash(tx).await.unwrap().unwrap();
        let overwrite_chain_id = Some(L2ChainId(tx_details.chain_id.as_u32() as u16));
        let miniblock_number = MiniblockNumber(tx_details.block_number.unwrap().as_u32());
        // We have to sync to the one-miniblock before the one where transaction is.
        let l2_miniblock = miniblock_number.saturating_sub(1) as u64;

        Self::from_url_and_miniblock_and_chain(url, client, l2_miniblock, overwrite_chain_id).await
    }

    /// Return URL and HTTP client for a given fork name.
    pub fn fork_to_url_and_client(fork: &str) -> (&str, HttpClient) {
        let url = match fork {
            "mainnet" => "https://mainnet.era.zksync.io:443",
            "testnet" => "https://testnet.era.zksync.dev:443",
            _ => fork,
        };

        let client = HttpClientBuilder::default()
            .build(url)
            .expect("Unable to create a client for fork");

        (url, client)
    }

    /// Returns transactions that are in the same L2 miniblock as replay_tx, but were executed before it.
    pub async fn get_earlier_transactions_in_same_block(&self, replay_tx: H256) -> Vec<L2Tx> {
        let client = self.create_client();

        let tx_details = client
            .get_transaction_by_hash(replay_tx)
            .await
            .unwrap()
            .unwrap();
        let miniblock = MiniblockNumber(tx_details.block_number.unwrap().as_u32());

        // And we're fetching all the transactions from this miniblock.
        let block_transactions: Vec<zksync_types::Transaction> =
            client.get_raw_block_transactions(miniblock).await.unwrap();
        let mut tx_to_apply = Vec::new();

        for tx in block_transactions {
            let h = tx.hash();
            let l2_tx: L2Tx = tx.try_into().unwrap();
            tx_to_apply.push(l2_tx);

            if h == replay_tx {
                return tx_to_apply;
            }
        }
        panic!(
            "Cound not find tx {:?} in miniblock: {:?}",
            replay_tx, miniblock
        );
    }

    pub fn create_client(&self) -> HttpClient {
        HttpClientBuilder::default()
            .build(self.fork_url.clone())
            .expect("Unable to create a client for fork")
    }
}
