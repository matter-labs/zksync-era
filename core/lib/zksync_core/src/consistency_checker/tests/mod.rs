//! Tests for the consistency checker component.

use std::collections::HashMap;

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::StorageProcessor;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::{BlockGasCount, L1BatchHeader},
    commitment::L1BatchWithMetadata,
    Address, L2ChainId, ProtocolVersionId,
};

use super::*;
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::tests::create_l1_batch_metadata,
};

fn create_l1_batch_with_metadata(number: u32) -> L1BatchWithMetadata {
    let mut header = L1BatchHeader::new(
        L1BatchNumber(number),
        number.into(),
        Address::default(),
        BaseSystemContractsHashes::default(),
        ProtocolVersionId::latest(),
    );
    header.is_finished = true;

    L1BatchWithMetadata {
        header,
        metadata: create_l1_batch_metadata(number),
        factory_deps: vec![],
    }
}

#[derive(Debug, Default)]
struct MockL1Client {
    transaction_status_responses: HashMap<H256, U64>,
    transaction_input_data_responses: HashMap<H256, Vec<u8>>,
}

impl MockL1Client {
    fn build_commit_tx_input_data(batches: &[L1BatchWithMetadata]) -> Vec<u8> {
        let commit_tokens = batches.iter().map(L1BatchWithMetadata::l1_commit_data);
        let commit_tokens = ethabi::Token::Array(commit_tokens.collect());

        let mut encoded = vec![];
        encoded.extend_from_slice(b"fake"); // Fake Solidity function selector (not checked for now)
                                            // Mock an additional arg used in real `commitBlocks` / `commitBatches`. In real transactions,
                                            // it's taken from the L1 batch previous to batches[0], but since this arg is not checked,
                                            // it's OK to use batches[0].
        let prev_header_tokens = batches[0].l1_header_data();
        encoded.extend_from_slice(&ethabi::encode(&[prev_header_tokens, commit_tokens]));
        encoded
    }
}

#[async_trait]
impl L1Client for MockL1Client {
    async fn transaction_status(&self, tx_hash: H256) -> Result<Option<U64>, web3::Error> {
        if let Some(response) = self.transaction_status_responses.get(&tx_hash) {
            return Ok(Some(*response));
        }
        Ok(None)
    }

    async fn transaction_input_data(&self, tx_hash: H256) -> Result<Option<Vec<u8>>, web3::Error> {
        if let Some(response) = self.transaction_input_data_responses.get(&tx_hash) {
            return Ok(Some(response.clone()));
        }
        Ok(None)
    }
}

impl UpdateCheckedBatch for mpsc::UnboundedSender<L1BatchNumber> {
    fn update_checked_batch(&mut self, last_checked_batch: L1BatchNumber) {
        self.send(last_checked_batch).ok();
    }
}

#[test]
fn build_commit_tx_input_data_is_correct() {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    let batches = vec![
        create_l1_batch_with_metadata(1),
        create_l1_batch_with_metadata(2),
    ];

    let commit_tx_input_data = MockL1Client::build_commit_tx_input_data(&batches);

    for batch in &batches {
        let commit_data = ConsistencyChecker::extract_commit_data(
            &commit_tx_input_data,
            commit_function,
            batch.header.number,
        )
        .unwrap();
        assert_eq!(commit_data, batch.l1_commit_data());
    }
}

#[test]
fn extracting_commit_data_for_boojum_batch() {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    // Calldata taken from the commit transaction for https://sepolia.explorer.zksync.io/batch/4470;
    // https://sepolia.etherscan.io/tx/0x300b9115037028b1f8aa2177abf98148c3df95c9b04f95a4e25baf4dfee7711f
    let commit_tx_input_data = include_bytes!("commit_l1_batch_4470_testnet_sepolia.calldata");

    let commit_data = ConsistencyChecker::extract_commit_data(
        commit_tx_input_data,
        commit_function,
        L1BatchNumber(4_470),
    )
    .unwrap();

    assert_matches!(
        commit_data,
        ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(4_470.into())
    );

    for bogus_l1_batch in [0, 1, 1_000, 4_469, 4_471, 100_000] {
        ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(bogus_l1_batch),
        )
        .unwrap_err();
    }
}

#[test]
fn extracting_commit_data_for_multiple_batches() {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    // Calldata taken from the commit transaction for https://explorer.zksync.io/batch/351000;
    // https://etherscan.io/tx/0xbd8dfe0812df0da534eb95a2d2a4382d65a8172c0b648a147d60c1c2921227fd
    let commit_tx_input_data = include_bytes!("commit_l1_batch_351000-351004_mainnet.calldata");

    for l1_batch in 351_000..=351_004 {
        let commit_data = ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(l1_batch),
        )
        .unwrap();

        assert_matches!(
            commit_data,
            ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(l1_batch.into())
        );
    }

    for bogus_l1_batch in [350_000, 350_999, 351_005, 352_000] {
        ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(bogus_l1_batch),
        )
        .unwrap_err();
    }
}

#[test]
fn extracting_commit_data_for_pre_boojum_batch() {
    // Calldata taken from the commit transaction for https://goerli.explorer.zksync.io/batch/200000;
    // https://goerli.etherscan.io/tx/0xfd2ef4ccd1223f502cc4a4e0f76c6905feafabc32ba616e5f70257eb968f20a3
    let commit_tx_input_data = include_bytes!("commit_l1_batch_200000_testnet_goerli.calldata");

    let commit_data = ConsistencyChecker::extract_commit_data(
        commit_tx_input_data,
        &PRE_BOOJUM_COMMIT_FUNCTION,
        L1BatchNumber(200_000),
    )
    .unwrap();

    assert_matches!(
        commit_data,
        ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(200_000.into())
    );
}

#[derive(Debug)]
enum SaveAction<'a> {
    InsertBatch(&'a L1BatchWithMetadata),
    SaveMetadata(&'a L1BatchWithMetadata),
    InsertCommitTx(L1BatchNumber),
}

impl SaveAction<'_> {
    async fn apply(
        self,
        storage: &mut StorageProcessor<'_>,
        commit_tx_hash_by_l1_batch: &HashMap<L1BatchNumber, H256>,
    ) {
        match self {
            Self::InsertBatch(l1_batch) => {
                storage
                    .blocks_dal()
                    .insert_l1_batch(&l1_batch.header, &[], BlockGasCount::default(), &[], &[])
                    .await
                    .unwrap();
            }
            Self::SaveMetadata(l1_batch) => {
                storage
                    .blocks_dal()
                    .save_l1_batch_metadata(
                        l1_batch.header.number,
                        &l1_batch.metadata,
                        H256::default(),
                        false,
                    )
                    .await
                    .unwrap();
            }
            Self::InsertCommitTx(l1_batch_number) => {
                let commit_tx_hash = commit_tx_hash_by_l1_batch[&l1_batch_number];
                storage
                    .eth_sender_dal()
                    .insert_bogus_confirmed_eth_tx(
                        l1_batch_number,
                        AggregatedActionType::Commit,
                        commit_tx_hash,
                        chrono::Utc::now(),
                    )
                    .await
                    .unwrap();
            }
        }
    }
}

type SaveActionMapper = fn(&[L1BatchWithMetadata]) -> Vec<SaveAction<'_>>;

/// Various strategies to persist L1 batches in the DB. Strings are added for debugging failed test cases.
const SAVE_ACTION_MAPPERS: [(&str, SaveActionMapper); 4] = [
    ("sequential_metadata_first", |l1_batches| {
        l1_batches
            .iter()
            .flat_map(|batch| {
                [
                    SaveAction::InsertBatch(batch),
                    SaveAction::SaveMetadata(batch),
                    SaveAction::InsertCommitTx(batch.header.number),
                ]
            })
            .collect()
    }),
    ("sequential_commit_txs_first", |l1_batches| {
        l1_batches
            .iter()
            .flat_map(|batch| {
                [
                    SaveAction::InsertBatch(batch),
                    SaveAction::InsertCommitTx(batch.header.number),
                    SaveAction::SaveMetadata(batch),
                ]
            })
            .collect()
    }),
    ("all_metadata_first", |l1_batches| {
        let commit_tx_actions = l1_batches
            .iter()
            .map(|batch| SaveAction::InsertCommitTx(batch.header.number));
        l1_batches
            .iter()
            .map(SaveAction::InsertBatch)
            .chain(l1_batches.iter().map(SaveAction::SaveMetadata))
            .chain(commit_tx_actions)
            .collect()
    }),
    ("all_commit_txs_first", |l1_batches| {
        let commit_tx_actions = l1_batches
            .iter()
            .map(|batch| SaveAction::InsertCommitTx(batch.header.number));
        l1_batches
            .iter()
            .map(SaveAction::InsertBatch)
            .chain(commit_tx_actions)
            .chain(l1_batches.iter().map(SaveAction::SaveMetadata))
            .collect()
    }),
];

#[test_casing(12, Product(([10, 3, 1], SAVE_ACTION_MAPPERS)))]
#[tokio::test]
async fn normal_checker_function(
    batches_per_transaction: usize,
    (mapper_name, save_actions_mapper): (&'static str, SaveActionMapper),
) {
    println!("Using save_actions_mapper={mapper_name}");

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let l1_batches: Vec<_> = (1..=10).map(create_l1_batch_with_metadata).collect();
    let mut commit_tx_hash_by_l1_batch = HashMap::with_capacity(l1_batches.len());
    let commit_transactions: Vec<_> = l1_batches
        .chunks(batches_per_transaction)
        .map(|l1_batches| {
            let tx_hash = H256::random();
            commit_tx_hash_by_l1_batch.extend(
                l1_batches
                    .iter()
                    .map(|batch| (batch.header.number, tx_hash)),
            );
            let input_data = MockL1Client::build_commit_tx_input_data(l1_batches);
            (tx_hash, input_data)
        })
        .collect();

    let mut client = MockL1Client::default();
    for (tx_hash, input_data) in &commit_transactions {
        client
            .transaction_status_responses
            .insert(*tx_hash, 1.into());
        client
            .transaction_input_data_responses
            .insert(*tx_hash, input_data.clone());
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        contract: zksync_contracts::zksync_contract(),
        max_batches_to_recheck: 100,
        sleep_interval: Duration::from_millis(10),
        l1_client: Box::new(client),
        l1_batch_updater: Box::new(l1_batch_updates_sender),
        pool: pool.clone(),
    };

    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    // Add new batches to the storage.
    for save_action in save_actions_mapper(&l1_batches) {
        save_action
            .apply(&mut storage, &commit_tx_hash_by_l1_batch)
            .await;
        tokio::time::sleep(Duration::from_millis(7)).await;
    }

    // Wait until all batches are checked.
    loop {
        let checked_batch = l1_batch_updates_receiver.recv().await.unwrap();
        if checked_batch == l1_batches.last().unwrap().header.number {
            break;
        }
    }

    // Send the stop signal to the checker and wait for it to stop.
    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

// FIXME: test (missing / wrong) (tx status / data)
