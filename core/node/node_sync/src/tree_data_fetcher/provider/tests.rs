//! Tests for tree data providers.

use once_cell::sync::Lazy;
use test_casing::test_casing;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_web3_decl::client::MockClient;

use super::*;
use crate::tree_data_fetcher::tests::seal_l1_batch_with_timestamp;

const DIAMOND_PROXY_ADDRESS: Address = Address::repeat_byte(0x22);

static BLOCK_COMMIT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    zksync_contracts::hyperchain_contract()
        .event("BlockCommit")
        .expect("missing `BlockCommit` event")
        .signature()
});

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
        let block_number = self.block_number;

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
                Ok(Self::filter_logs(&logs, filter))
            })
            .build()
    }
}

#[tokio::test]
async fn guessing_l1_commit_block_number() {
    let eth_params = EthereumParameters::new(100_000);
    let eth_client = eth_params.client();

    for timestamp in [0, 100, 1_000, 5_000, 10_000, 100_000] {
        let guessed_block_number =
            L1DataProvider::guess_l1_commit_block_number(&eth_client, timestamp)
                .await
                .unwrap();

        assert!(
            guessed_block_number.abs_diff(timestamp.into()) <= L1DataProvider::L1_BLOCK_ACCURACY,
            "timestamp={timestamp}, guessed={guessed_block_number}"
        );
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
    drop(storage);

    let mut provider =
        L1DataProvider::new(pool, Box::new(eth_params.client()), DIAMOND_PROXY_ADDRESS).unwrap();
    for i in 0..l1_batch_timestamps.len() {
        let number = L1BatchNumber(i as u32 + 1);
        let root_hash = provider
            .batch_details(number)
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
