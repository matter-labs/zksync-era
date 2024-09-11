//! Tests for tree data providers.

use assert_matches::assert_matches;
use once_cell::sync::Lazy;
use test_casing::test_casing;
use zksync_dal::{ConnectionPool, Core};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::create_l2_block;
use zksync_types::{api, L2BlockNumber, ProtocolVersionId};
use zksync_web3_decl::client::MockClient;

use super::*;
use crate::tree_data_fetcher::tests::{
    get_last_l2_block, seal_l1_batch_with_timestamp, MockMainNodeClient,
};

const DIAMOND_PROXY_ADDRESS: Address = Address::repeat_byte(0x22);

static BLOCK_COMMIT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    zksync_contracts::hyperchain_contract()
        .event("BlockCommit")
        .expect("missing `BlockCommit` event")
        .signature()
});

fn mock_block_details_base(number: u32, hash: Option<H256>) -> api::BlockDetailsBase {
    api::BlockDetailsBase {
        timestamp: number.into(),
        root_hash: hash,
        // The fields below are not read.
        l1_tx_count: 0,
        l2_tx_count: 1,
        status: api::BlockStatus::Sealed,
        commit_tx_hash: None,
        committed_at: None,
        prove_tx_hash: None,
        proven_at: None,
        execute_tx_hash: None,
        executed_at: None,
        l1_gas_price: 10,
        l2_fair_gas_price: 100,
        fair_pubdata_price: None,
        base_system_contracts_hashes: Default::default(),
    }
}

#[derive(Debug)]
struct L2Parameters {
    l2_block_hashes: Vec<H256>,
    l1_batch_root_hashes: Vec<H256>,
}

impl L2Parameters {
    fn mock_client(self) -> MockClient<L2> {
        let block_number = U64::from(self.l2_block_hashes.len());

        MockClient::builder(L2::default())
            .method("eth_blockNumber", move || Ok(block_number))
            .method("zks_getL1BatchDetails", move |number: L1BatchNumber| {
                let root_hash = self.l1_batch_root_hashes.get(number.0 as usize);
                Ok(root_hash.map(|&hash| api::L1BatchDetails {
                    number,
                    base: mock_block_details_base(number.0, Some(hash)),
                }))
            })
            .method("zks_getBlockDetails", move |number: L2BlockNumber| {
                let hash = self.l2_block_hashes.get(number.0 as usize);
                Ok(hash.map(|&hash| api::BlockDetails {
                    number,
                    l1_batch_number: L1BatchNumber(number.0),
                    operator_address: Address::zero(),
                    protocol_version: Some(ProtocolVersionId::latest()),
                    base: mock_block_details_base(number.0, Some(hash)),
                }))
            })
            .build()
    }
}

#[tokio::test]
async fn rpc_data_provider_basics() {
    let last_l2_block = create_l2_block(1);
    let l2_parameters = L2Parameters {
        l2_block_hashes: vec![H256::zero(), last_l2_block.hash],
        l1_batch_root_hashes: vec![H256::zero(), H256::from_low_u64_be(1)],
    };
    let mut client: Box<DynClient<L2>> = Box::new(l2_parameters.mock_client());

    let root_hash = client
        .batch_details(L1BatchNumber(1), &last_l2_block)
        .await
        .unwrap()
        .expect("missing block");
    assert_eq!(root_hash, H256::from_low_u64_be(1));

    // Query a future L1 batch.
    let output = client
        .batch_details(L1BatchNumber(2), &create_l2_block(2))
        .await
        .unwrap();
    assert_matches!(output, Err(MissingData::Batch));
}

#[tokio::test]
async fn rpc_data_provider_with_block_hash_divergence() {
    let last_l2_block = create_l2_block(1);
    let l2_parameters = L2Parameters {
        l2_block_hashes: vec![H256::zero(), H256::repeat_byte(1)], // Hash for block #1 differs from the local one
        l1_batch_root_hashes: vec![H256::zero(), H256::from_low_u64_be(1)],
    };
    let mut client: Box<DynClient<L2>> = Box::new(l2_parameters.mock_client());

    let output = client
        .batch_details(L1BatchNumber(1), &last_l2_block)
        .await
        .unwrap();
    assert_matches!(output, Err(MissingData::PossibleReorg));
}

struct EthereumParameters {
    block_number: U64,
    // L1 block numbers in which L1 batches are committed starting from L1 batch #1
    l1_blocks_for_commits: Vec<U64>,
}

impl EthereumParameters {
    fn new(block_number: u64) -> Self {
        Self {
            block_number: block_number.into(),
            l1_blocks_for_commits: vec![],
        }
    }

    fn push_commit(&mut self, l1_block_number: u64) {
        assert!(l1_block_number <= self.block_number.as_u64());

        let l1_block_number = U64::from(l1_block_number);
        let last_commit = self.l1_blocks_for_commits.last().copied();
        let is_increasing = last_commit.map_or(true, |last_number| last_number <= l1_block_number);
        assert!(is_increasing, "Invalid L1 block number for commit");

        self.l1_blocks_for_commits.push(l1_block_number);
    }

    fn client(&self) -> MockClient<L1> {
        let logs = self
            .l1_blocks_for_commits
            .iter()
            .enumerate()
            .map(|(i, &l1_block_number)| {
                let l1_batch_number = H256::from_low_u64_be(i as u64 + 1);
                let root_hash = H256::repeat_byte(i as u8 + 1);
                web3::Log {
                    address: DIAMOND_PROXY_ADDRESS,
                    topics: vec![
                        *BLOCK_COMMIT_SIGNATURE,
                        l1_batch_number,
                        root_hash,
                        H256::zero(), // commitment hash; not used
                    ],
                    block_number: Some(l1_block_number),
                    ..web3::Log::default()
                }
            });
        let logs: Vec<_> = logs.collect();
        mock_l1_client(self.block_number, logs)
    }
}

fn filter_logs(logs: &[web3::Log], filter: web3::Filter) -> Vec<web3::Log> {
    let Some(web3::BlockNumber::Number(filter_from)) = filter.from_block else {
        panic!("Unexpected filter: {filter:?}");
    };
    let Some(web3::BlockNumber::Number(filter_to)) = filter.to_block else {
        panic!("Unexpected filter: {filter:?}");
    };
    let filter_block_range = filter_from..=filter_to;

    let filter_addresses = filter.address.unwrap().flatten();
    let filter_topics = filter.topics.unwrap();
    let filter_topics: Vec<_> = filter_topics
        .into_iter()
        .map(|topic| topic.map(web3::ValueOrArray::flatten))
        .collect();

    let filtered_logs = logs.iter().filter(|log| {
        if !filter_addresses.contains(&log.address) {
            return false;
        }
        if !filter_block_range.contains(&log.block_number.unwrap()) {
            return false;
        }
        filter_topics
            .iter()
            .zip(&log.topics)
            .all(|(filter_topics, actual_topic)| match filter_topics {
                Some(topics) => topics.contains(actual_topic),
                None => true,
            })
    });
    filtered_logs.cloned().collect()
}

fn mock_l1_client(block_number: U64, logs: Vec<web3::Log>) -> MockClient<L1> {
    MockClient::builder(L1::default())
        .method("eth_blockNumber", move || Ok(block_number))
        .method(
            "eth_getBlockByNumber",
            move |number: web3::BlockNumber, with_txs: bool| {
                assert!(!with_txs);

                let number = match number {
                    web3::BlockNumber::Number(number) => number,
                    web3::BlockNumber::Latest => block_number,
                    web3::BlockNumber::Earliest => U64::zero(),
                    _ => panic!("Unexpected number: {number:?}"),
                };
                if number > block_number {
                    return Ok(None);
                }
                Ok(Some(web3::Block::<H256> {
                    number: Some(number),
                    timestamp: U256::from(number.as_u64()), // timestamp == number
                    ..web3::Block::default()
                }))
            },
        )
        .method("eth_getLogs", move |filter: web3::Filter| {
            Ok(filter_logs(&logs, filter))
        })
        .build()
}

#[tokio::test]
async fn guessing_l1_commit_block_number() {
    let eth_params = EthereumParameters::new(100_000);
    let eth_client = eth_params.client();

    for timestamp in [0, 100, 1_000, 5_000, 10_000, 100_000] {
        let (guessed_block_number, step_count) =
            L1DataProvider::guess_l1_commit_block_number(&eth_client, timestamp)
                .await
                .unwrap();

        assert!(
            guessed_block_number.abs_diff(timestamp.into()) <= L1DataProvider::L1_BLOCK_ACCURACY,
            "timestamp={timestamp}, guessed={guessed_block_number}"
        );
        assert!(step_count > 0);
        assert!(step_count < 100);
    }
}

async fn test_using_l1_data_provider(l1_batch_timestamps: &[u64]) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let mut eth_params = EthereumParameters::new(1_000_000);
    for (number, &ts) in l1_batch_timestamps.iter().enumerate() {
        let number = L1BatchNumber(number as u32 + 1);
        seal_l1_batch_with_timestamp(&mut storage, number, ts).await;
        eth_params.push_commit(ts + 1_000); // have a reasonable small diff between batch generation and commitment
    }

    let mut provider =
        L1DataProvider::new(Box::new(eth_params.client()), DIAMOND_PROXY_ADDRESS).unwrap();
    for i in 0..l1_batch_timestamps.len() {
        let number = L1BatchNumber(i as u32 + 1);
        let root_hash = provider
            .batch_details(number, &get_last_l2_block(&mut storage, number).await)
            .await
            .unwrap()
            .expect("no root hash");
        assert_eq!(root_hash, H256::repeat_byte(number.0 as u8));

        let past_l1_batch = provider.past_l1_batch.unwrap();
        assert_eq!(past_l1_batch.number, number);
        let expected_l1_block_number = eth_params.l1_blocks_for_commits[i];
        assert_eq!(
            past_l1_batch.l1_commit_block_number,
            expected_l1_block_number
        );
        assert_eq!(
            past_l1_batch.l1_commit_block_timestamp,
            expected_l1_block_number.as_u64().into()
        );
    }
}

#[test_casing(4, [500, 1_500, 10_000, 30_000])]
#[tokio::test]
async fn using_l1_data_provider(batch_spacing: u64) {
    let l1_batch_timestamps: Vec<_> = (0..10).map(|i| 50_000 + batch_spacing * i).collect();
    test_using_l1_data_provider(&l1_batch_timestamps).await;
}

#[tokio::test]
async fn detecting_reorg_in_l1_data_provider() {
    let l1_batch_number = H256::from_low_u64_be(1);
    // Generate two logs for the same L1 batch #1
    let logs = vec![
        web3::Log {
            address: DIAMOND_PROXY_ADDRESS,
            topics: vec![
                *BLOCK_COMMIT_SIGNATURE,
                l1_batch_number,
                H256::repeat_byte(1),
                H256::zero(), // commitment hash; not used
            ],
            block_number: Some(1.into()),
            ..web3::Log::default()
        },
        web3::Log {
            address: DIAMOND_PROXY_ADDRESS,
            topics: vec![
                *BLOCK_COMMIT_SIGNATURE,
                l1_batch_number,
                H256::repeat_byte(2),
                H256::zero(), // commitment hash; not used
            ],
            block_number: Some(100.into()),
            ..web3::Log::default()
        },
    ];
    let l1_client = mock_l1_client(200.into(), logs);

    let mut provider = L1DataProvider::new(Box::new(l1_client), DIAMOND_PROXY_ADDRESS).unwrap();
    let output = provider
        .batch_details(L1BatchNumber(1), &create_l2_block(1))
        .await
        .unwrap();
    assert_matches!(output, Err(MissingData::PossibleReorg));
}

#[tokio::test]
async fn combined_data_provider_errors() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let mut eth_params = EthereumParameters::new(1_000_000);
    seal_l1_batch_with_timestamp(&mut storage, L1BatchNumber(1), 50_000).await;
    eth_params.push_commit(51_000);
    seal_l1_batch_with_timestamp(&mut storage, L1BatchNumber(2), 52_000).await;

    let mut main_node_client = MockMainNodeClient::default();
    main_node_client.insert_batch(L1BatchNumber(2), H256::repeat_byte(2));
    let mut provider = CombinedDataProvider::new(main_node_client);
    let l1_provider =
        L1DataProvider::new(Box::new(eth_params.client()), DIAMOND_PROXY_ADDRESS).unwrap();
    provider.set_l1(l1_provider);

    // L1 batch #1 should be obtained from L1
    let root_hash = provider
        .batch_details(
            L1BatchNumber(1),
            &get_last_l2_block(&mut storage, L1BatchNumber(1)).await,
        )
        .await
        .unwrap()
        .expect("no root hash");
    assert_eq!(root_hash, H256::repeat_byte(1));
    assert!(provider.l1.is_some());

    // L1 batch #2 should be obtained from L2
    let root_hash = provider
        .batch_details(
            L1BatchNumber(2),
            &get_last_l2_block(&mut storage, L1BatchNumber(2)).await,
        )
        .await
        .unwrap()
        .expect("no root hash");
    assert_eq!(root_hash, H256::repeat_byte(2));
    assert!(provider.l1.is_none());
}
