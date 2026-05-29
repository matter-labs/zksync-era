//! Tests for tree data providers.

use assert_matches::assert_matches;
use once_cell::sync::Lazy;
use test_casing::test_casing;
use zksync_contracts::bridgehub_contract;
use zksync_dal::{ConnectionPool, Core};
use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
use zksync_node_test_utils::create_l2_block;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_types::{
    api, ethabi,
    web3::{BlockId, CallRequest},
    L2BlockNumber, ProtocolVersionId,
};
use zksync_web3_decl::client::{MockClient, L1};

use super::*;
use crate::tree_data_fetcher::tests::{
    get_last_l2_block, seal_l1_batch_with_timestamp, MockMainNodeClient,
};

const L1_DIAMOND_PROXY_ADDRESS: Address = Address::repeat_byte(0x22);
const GATEWAY_DIAMOND_PROXY_ADDRESS: Address = Address::repeat_byte(0x33);
const L1_CHAIN_ID: u64 = 9;
const ERA_CHAIN_ID: u64 = 270;

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
        commit_tx_finality: None,
        commit_chain_id: None,
        prove_tx_hash: None,
        prove_tx_finality: None,
        proven_at: None,
        prove_chain_id: None,
        execute_tx_hash: None,
        execute_tx_finality: None,
        executed_at: None,
        execute_chain_id: None,
        precommit_tx_hash: None,
        precommit_tx_finality: None,
        precommitted_at: None,
        precommit_chain_id: None,
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
                    commitment: root_hash.copied(),
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

#[derive(Debug)]
struct EthereumParameters {
    block_number: U64,
    // L1 batch numbers and SL block numbers in which they are committed.
    batches_and_sl_blocks_for_commits: Vec<(L1BatchNumber, U64)>,
    chain_id: SLChainId,
    diamond_proxy: Address,
}

impl EthereumParameters {
    fn new_l1(block_number: u64) -> Self {
        Self::new(block_number, L1_CHAIN_ID, L1_DIAMOND_PROXY_ADDRESS)
    }

    fn new(block_number: u64, chain_id: u64, diamond_proxy: Address) -> Self {
        Self {
            block_number: block_number.into(),
            batches_and_sl_blocks_for_commits: vec![],
            chain_id: SLChainId(chain_id),
            diamond_proxy,
        }
    }

    fn push_commit(&mut self, l1_batch_number: L1BatchNumber, l1_block_number: u64) {
        assert!(l1_block_number <= self.block_number.as_u64());

        let l1_block_number = U64::from(l1_block_number);
        let last_commit = self.batches_and_sl_blocks_for_commits.last().copied();
        let is_increasing =
            last_commit.is_none_or(|last| last.0 <= l1_batch_number && last.1 <= l1_block_number);
        assert!(
            is_increasing,
            "Invalid batch number or L1 block number for commit"
        );

        self.batches_and_sl_blocks_for_commits
            .push((l1_batch_number, l1_block_number));
    }

    fn client(&self) -> MockClient<L1> {
        let logs = self.batches_and_sl_blocks_for_commits.iter().map(
            |&(l1_batch_number, l1_block_number)| {
                let l1_batch_number_topic = H256::from_low_u64_be(l1_batch_number.0 as u64);
                let root_hash = H256::repeat_byte(l1_batch_number.0 as u8);
                web3::Log {
                    address: self.diamond_proxy,
                    topics: vec![
                        *BLOCK_COMMIT_SIGNATURE,
                        l1_batch_number_topic,
                        root_hash,
                        H256::zero(), // commitment hash; not used
                    ],
                    block_number: Some(l1_block_number),
                    ..web3::Log::default()
                }
            },
        );
        let logs: Vec<_> = logs.collect();
        mock_l1_client(self.block_number, logs, self.chain_id)
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

fn mock_l1_client(block_number: U64, logs: Vec<web3::Log>, chain_id: SLChainId) -> MockClient<L1> {
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
        .method("eth_chainId", move || Ok(U64::from(chain_id.0)))
        .method("eth_call", move |req: CallRequest, _block_id: BlockId| {
            let contract = bridgehub_contract();
            let expected_input = contract
                .function("getZKChain")
                .unwrap()
                .encode_input(&[ethabi::Token::Uint(ERA_CHAIN_ID.into())])
                .unwrap();
            assert_eq!(req.to, Some(L2_BRIDGEHUB_ADDRESS));
            assert_eq!(req.data, Some(expected_input.into()));
            Ok(web3::Bytes(ethabi::encode(&[ethabi::Token::Address(
                GATEWAY_DIAMOND_PROXY_ADDRESS,
            )])))
        })
        .build()
}

#[tokio::test]
async fn guessing_l1_commit_block_number() {
    let eth_params = EthereumParameters::new_l1(100_000);
    let eth_client = eth_params.client();

    for timestamp in [0, 100, 1_000, 5_000, 10_000, 100_000] {
        let (guessed_block_number, step_count) =
            SLDataProvider::guess_l1_commit_block_number(&eth_client, timestamp)
                .await
                .unwrap();

        assert!(
            guessed_block_number.abs_diff(timestamp.into()) <= SLDataProvider::L1_BLOCK_ACCURACY,
            "timestamp={timestamp}, guessed={guessed_block_number}"
        );
        assert!(step_count > 0);
        assert!(step_count < 100);
    }
}

async fn create_l1_data_provider(l1_client: Box<DynClient<L1>>) -> SLDataProvider {
    SLDataProvider::new(Box::new(l1_client), L1_DIAMOND_PROXY_ADDRESS)
        .await
        .unwrap()
}

async fn test_using_l1_data_provider(l1_batch_timestamps: &[u64]) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let mut eth_params = EthereumParameters::new_l1(1_000_000);
    for (number, &ts) in l1_batch_timestamps.iter().enumerate() {
        let number = L1BatchNumber(number as u32 + 1);
        seal_l1_batch_with_timestamp(&mut storage, number, ts).await;
        eth_params.push_commit(number, ts + 1_000); // have a reasonable small diff between batch generation and commitment
    }

    let mut provider = create_l1_data_provider(Box::new(eth_params.client())).await;
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
        let expected_l1_block_number = eth_params.batches_and_sl_blocks_for_commits[i].1;
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
            address: L1_DIAMOND_PROXY_ADDRESS,
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
            address: L1_DIAMOND_PROXY_ADDRESS,
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
    let l1_client = mock_l1_client(200.into(), logs, SLChainId(9));

    let mut provider = create_l1_data_provider(Box::new(l1_client)).await;
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
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let mut eth_params = EthereumParameters::new_l1(1_000_000);
    seal_l1_batch_with_timestamp(&mut storage, L1BatchNumber(1), 50_000).await;
    eth_params.push_commit(L1BatchNumber(1), 51_000);
    seal_l1_batch_with_timestamp(&mut storage, L1BatchNumber(2), 52_000).await;

    let mut main_node_client = MockMainNodeClient::default();
    main_node_client.insert_batch(L1BatchNumber(2), H256::repeat_byte(2));
    let mut provider = CombinedDataProvider::new(main_node_client);
    let l1_provider = create_l1_data_provider(Box::new(eth_params.client())).await;
    provider.set_sl(l1_provider);

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
