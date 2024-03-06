//! Tests for the consistency checker component.
mod test_helpers;

use std::{collections::HashMap, slice};

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_dal::StorageProcessor;
use zksync_eth_client::clients::MockEthereum;
use zksync_types::{
    aggregated_operations::AggregatedActionType, commitment::L1BatchWithMetadata,
    web3::contract::Options, ProtocolVersionId, H256,
};

use super::*;
use crate::{
    eth_sender::l1_batch_commit_data_generator::{
        RollupModeL1BatchCommitDataGenerator, ValidiumModeL1BatchCommitDataGenerator,
    },
    utils::testonly::{
        create_l1_batch, create_l1_batch_metadata, l1_batch_metadata_to_commitment_artifacts,
    },
};

/// **NB.** For tests to run correctly, the returned value must be deterministic (i.e., depend only on `number`).
pub(crate) fn create_l1_batch_with_metadata(number: u32) -> L1BatchWithMetadata {
    L1BatchWithMetadata {
        header: create_l1_batch(number),
        metadata: create_l1_batch_metadata(number),
        raw_published_factory_deps: vec![],
    }
}

const PRE_BOOJUM_PROTOCOL_VERSION: ProtocolVersionId = ProtocolVersionId::Version10;

pub(crate) fn create_pre_boojum_l1_batch_with_metadata(number: u32) -> L1BatchWithMetadata {
    let mut l1_batch = L1BatchWithMetadata {
        header: create_l1_batch(number),
        metadata: create_l1_batch_metadata(number),
        raw_published_factory_deps: vec![],
    };
    l1_batch.header.protocol_version = Some(PRE_BOOJUM_PROTOCOL_VERSION);
    l1_batch.metadata.bootloader_initial_content_commitment = None;
    l1_batch.metadata.events_queue_commitment = None;
    l1_batch
}

pub(crate) fn build_commit_tx_input_data(
    batches: &[L1BatchWithMetadata],
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) -> Vec<u8> {
    let mut encoded = vec![];
    // Fake Solidity function selector (not checked for now)
    encoded.extend_from_slice(b"fake");
    // Mock an additional argument used in real `commitBlocks` / `commitBatches`. In real transactions,
    // it's taken from the L1 batch previous to `batches[0]`, but since this argument is not checked,
    // it's OK to use `batches[0]`.
    encoded.extend_from_slice(&ethabi::encode(
        &l1_batch_commit_data_generator.l1_commit_batches(&batches[0], batches),
    ));
    encoded
}

pub(crate) fn create_mock_checker(
    client: MockEthereum,
    pool: ConnectionPool,
    l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
) -> ConsistencyChecker {
    let (health_check, health_updater) = ConsistencyCheckerHealthUpdater::new();
    ConsistencyChecker {
        contract: zksync_contracts::zksync_contract(),
        max_batches_to_recheck: 100,
        sleep_interval: Duration::from_millis(10),
        l1_client: Box::new(client),
        event_handler: Box::new(health_updater),
        l1_data_mismatch_behavior: L1DataMismatchBehavior::Bail,
        pool,
        l1_batch_commit_data_generator,
        health_check,
    }
}

impl HandleConsistencyCheckerEvent for mpsc::UnboundedSender<L1BatchNumber> {
    fn initialize(&mut self) {
        // Do nothing
    }

    fn set_first_batch_to_check(&mut self, _first_batch_to_check: L1BatchNumber) {
        // Do nothing
    }

    fn update_checked_batch(&mut self, last_checked_batch: L1BatchNumber) {
        self.send(last_checked_batch).ok();
    }

    fn report_inconsistent_batch(&mut self, _number: L1BatchNumber) {
        // Do nothing
    }
}

#[test]
fn build_commit_tx_input_data_is_correct() {
    test_helpers::build_commit_tx_input_data_is_correct(Arc::new(
        RollupModeL1BatchCommitDataGenerator {},
    ));
    test_helpers::build_commit_tx_input_data_is_correct(Arc::new(
        ValidiumModeL1BatchCommitDataGenerator {},
    ));
}

#[test]
fn extracting_commit_data_for_boojum_batch() {
    let contract = zksync_contracts::zksync_contract();
    let commit_function = contract.function("commitBatches").unwrap();
    // Calldata taken from the commit transaction for `https://sepolia.explorer.zksync.io/batch/4470`;
    // `https://sepolia.etherscan.io/tx/0x300b9115037028b1f8aa2177abf98148c3df95c9b04f95a4e25baf4dfee7711f`
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
    // Calldata taken from the commit transaction for `https://explorer.zksync.io/batch/351000`;
    // `https://etherscan.io/tx/0xbd8dfe0812df0da534eb95a2d2a4382d65a8172c0b648a147d60c1c2921227fd`
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
    // Calldata taken from the commit transaction for `https://goerli.explorer.zksync.io/batch/200000`;
    // `https://goerli.etherscan.io/tx/0xfd2ef4ccd1223f502cc4a4e0f76c6905feafabc32ba616e5f70257eb968f20a3`
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

#[derive(Debug, Clone, Copy)]
pub(crate) enum SaveAction<'a> {
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
                    .insert_mock_l1_batch(&l1_batch.header)
                    .await
                    .unwrap();
            }
            Self::SaveMetadata(l1_batch) => {
                storage
                    .blocks_dal()
                    .save_l1_batch_tree_data(l1_batch.header.number, &l1_batch.metadata.tree_data())
                    .await
                    .unwrap();
                storage
                    .blocks_dal()
                    .save_l1_batch_commitment_artifacts(
                        l1_batch.header.number,
                        &l1_batch_metadata_to_commitment_artifacts(&l1_batch.metadata),
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

pub(crate) type SaveActionMapper = fn(&[L1BatchWithMetadata]) -> Vec<SaveAction<'_>>;

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
    test_helpers::normal_checker_function(
        batches_per_transaction,
        (mapper_name, save_actions_mapper),
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    test_helpers::normal_checker_function(
        batches_per_transaction,
        (mapper_name, save_actions_mapper),
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;
}

#[test_casing(4, SAVE_ACTION_MAPPERS)]
#[tokio::test]
async fn checker_processes_pre_boojum_batches(
    (mapper_name, save_actions_mapper): (&'static str, SaveActionMapper),
) {
    test_helpers::checker_processes_pre_boojum_batches(
        (mapper_name, save_actions_mapper),
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    test_helpers::checker_processes_pre_boojum_batches(
        (mapper_name, save_actions_mapper),
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn checker_functions_after_snapshot_recovery(delay_batch_insertion: bool) {
    test_helpers::checker_functions_after_snapshot_recovery(
        delay_batch_insertion,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    test_helpers::checker_functions_after_snapshot_recovery(
        delay_batch_insertion,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum IncorrectDataKind {
    MissingStatus,
    MismatchedStatus,
    BogusCommitDataFormat,
    MismatchedCommitDataTimestamp,
    CommitDataForAnotherBatch,
    CommitDataForPreBoojum,
}

impl IncorrectDataKind {
    const ALL: [Self; 6] = [
        Self::MissingStatus,
        Self::MismatchedStatus,
        Self::BogusCommitDataFormat,
        Self::MismatchedCommitDataTimestamp,
        Self::CommitDataForAnotherBatch,
        Self::CommitDataForPreBoojum,
    ];

    async fn apply(
        self,
        client: &MockEthereum,
        l1_batch: &L1BatchWithMetadata,
        l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
    ) -> H256 {
        let (commit_tx_input_data, successful_status) = match self {
            Self::MissingStatus => {
                return H256::zero(); // Do not execute the transaction
            }
            Self::MismatchedStatus => {
                let commit_tx_input_data = build_commit_tx_input_data(
                    slice::from_ref(l1_batch),
                    l1_batch_commit_data_generator,
                );
                (commit_tx_input_data, false)
            }
            Self::BogusCommitDataFormat => {
                let mut bogus_tx_input_data = b"test".to_vec(); // Preserve the function selector
                bogus_tx_input_data
                    .extend_from_slice(&ethabi::encode(&[ethabi::Token::Bool(true)]));
                (bogus_tx_input_data, true)
            }
            Self::MismatchedCommitDataTimestamp => {
                let mut l1_batch = create_l1_batch_with_metadata(1);
                l1_batch.header.timestamp += 1;
                let bogus_tx_input_data = build_commit_tx_input_data(
                    slice::from_ref(&l1_batch),
                    l1_batch_commit_data_generator,
                );
                (bogus_tx_input_data, true)
            }
            Self::CommitDataForAnotherBatch => {
                let l1_batch = create_l1_batch_with_metadata(100);
                let bogus_tx_input_data = build_commit_tx_input_data(
                    slice::from_ref(&l1_batch),
                    l1_batch_commit_data_generator,
                );
                (bogus_tx_input_data, true)
            }
            Self::CommitDataForPreBoojum => {
                let mut l1_batch = create_l1_batch_with_metadata(1);
                l1_batch.header.protocol_version = Some(ProtocolVersionId::Version0);
                let bogus_tx_input_data = build_commit_tx_input_data(
                    slice::from_ref(&l1_batch),
                    l1_batch_commit_data_generator,
                );
                (bogus_tx_input_data, true)
            }
        };

        let signed_tx = client.sign_prepared_tx(
            commit_tx_input_data,
            Options {
                nonce: Some(0.into()),
                ..Options::default()
            },
        );
        let signed_tx = signed_tx.unwrap();
        client.send_raw_tx(signed_tx.raw_tx).await.unwrap();
        client.execute_tx(signed_tx.hash, successful_status, 1);
        signed_tx.hash
    }
}

#[test_casing(6, Product((IncorrectDataKind::ALL, [false])))]
// ^ `snapshot_recovery = true` is tested below; we don't want to run it with all incorrect data kinds
#[tokio::test]
async fn checker_detects_incorrect_tx_data(kind: IncorrectDataKind, snapshot_recovery: bool) {
    test_helpers::checker_detects_incorrect_tx_data(
        kind,
        snapshot_recovery,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    test_helpers::checker_detects_incorrect_tx_data(
        kind,
        snapshot_recovery,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;
}

#[tokio::test]
async fn checker_detects_incorrect_tx_data_after_snapshot_recovery() {
    checker_detects_incorrect_tx_data(IncorrectDataKind::CommitDataForAnotherBatch, true).await;
}
