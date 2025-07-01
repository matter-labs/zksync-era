//! Tests for the consistency checker component.
use std::{collections::HashMap, slice};

use assert_matches::assert_matches;
use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_config::GenesisConfig;
use zksync_dal::Connection;
use zksync_eth_client::{clients::MockSettlementLayer, EthInterface, Options};
use zksync_l1_contract_interface::{i_executor::methods::CommitBatches, Tokenizable, Tokenize};
use zksync_node_genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams};
use zksync_node_test_utils::{
    create_l1_batch, create_l1_batch_metadata, create_l2_block,
    l1_batch_metadata_to_commitment_artifacts,
};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::L2BlockHeader,
    commitment::{L1BatchWithMetadata, PubdataType},
    eth_sender::EthTxFinalityStatus,
    protocol_version::ProtocolSemanticVersion,
    web3::Log,
    ProtocolVersion, ProtocolVersionId, H256, L2_BRIDGEHUB_ADDRESS,
};

use super::*;

/// **NB.** For tests to run correctly, the returned value must be deterministic (i.e., depend only on `number`).
pub(crate) fn create_l1_batch_with_metadata(number: u32) -> L1BatchWithMetadata {
    L1BatchWithMetadata {
        header: create_l1_batch(number),
        metadata: create_l1_batch_metadata(number),
        raw_published_factory_deps: vec![],
    }
}

fn batch_to_block_number(batch: &L1BatchWithMetadata) -> u32 {
    batch.header.number.0 * 1000
}

const PRE_BOOJUM_PROTOCOL_VERSION: ProtocolVersionId = ProtocolVersionId::Version10;
const L1_DIAMOND_PROXY_ADDR: Address = Address::repeat_byte(1);
const GATEWAY_DIAMOND_PROXY_ADDR: Address = Address::repeat_byte(2);
const VALIDATOR_TIMELOCK_ADDR: Address = Address::repeat_byte(23);
const ERA_CHAIN_ID: u64 = 270;
const L1_CHAIN_ID: u64 = 9;
const COMMITMENT_MODES: [L1BatchCommitmentMode; 2] = [
    L1BatchCommitmentMode::Rollup,
    L1BatchCommitmentMode::Validium,
];

pub(crate) fn create_pre_boojum_l1_batch_with_metadata(number: u32) -> L1BatchWithMetadata {
    let mut l1_batch = L1BatchWithMetadata {
        header: create_l1_batch(number),
        metadata: create_l1_batch_metadata(number),
        raw_published_factory_deps: vec![],
    };
    l1_batch.header.protocol_version = Some(PRE_BOOJUM_PROTOCOL_VERSION);
    l1_batch.header.l2_to_l1_logs = vec![];
    l1_batch.metadata.bootloader_initial_content_commitment = None;
    l1_batch.metadata.events_queue_commitment = None;
    l1_batch
}

pub(crate) fn build_commit_tx_input_data(
    batches: &[L1BatchWithMetadata],
    mode: L1BatchCommitmentMode,
) -> Vec<u8> {
    let protocol_version = batches[0].header.protocol_version.unwrap();
    let contract = zksync_contracts::hyperchain_contract();

    // Mock an additional argument used in real `commitBlocks` / `commitBatches`. In real transactions,
    // it's taken from the L1 batch previous to `batches[0]`, but since this argument is not checked,
    // it's OK to use `batches[0]`.
    let tokens = CommitBatches {
        last_committed_l1_batch: &batches[0],
        l1_batches: batches,
        pubdata_da: PubdataSendingMode::Calldata,
        mode,
    }
    .into_tokens();

    if protocol_version.is_pre_boojum() {
        PRE_BOOJUM_COMMIT_FUNCTION.encode_input(&tokens).unwrap()
    } else if protocol_version.is_pre_shared_bridge() {
        POST_BOOJUM_COMMIT_FUNCTION.encode_input(&tokens).unwrap()
    } else {
        // Post shared bridge transactions also require chain id
        let tokens: Vec<_> = vec![Token::Uint(ERA_CHAIN_ID.into())]
            .into_iter()
            .chain(tokens)
            .collect();
        contract
            .function("commitBatchesSharedBridge")
            .unwrap()
            .encode_input(&tokens)
            .unwrap()
    }
}

pub(crate) async fn create_mock_checker(
    client: MockSettlementLayer,
    pool: ConnectionPool<Core>,
) -> ConsistencyChecker {
    let (health_check, health_updater) = ConsistencyCheckerHealthUpdater::new();
    let client = client.into_client();
    let chain_id = client.fetch_chain_id().await.unwrap();
    let chain_data = SLChainAccess {
        client: Box::new(client),
        chain_id,
        diamond_proxy_addr: Some(L1_DIAMOND_PROXY_ADDR),
    };
    ConsistencyChecker {
        contract: zksync_contracts::hyperchain_contract(),
        max_batches_to_recheck: 100,
        sleep_interval: Duration::from_millis(10),
        chain_data,
        settlement_layer: SettlementLayer::L1(chain_id),
        event_handler: Box::new(health_updater),
        pool,
        health_check,
    }
}

fn create_mock_sl(chain_id: u64, with_get_zk_chain: bool) -> MockSettlementLayer {
    let mock = MockSettlementLayer::builder()
        .with_call_handler(move |call, _block_id| match call.to {
            Some(addr) if addr == L1_DIAMOND_PROXY_ADDR || addr == GATEWAY_DIAMOND_PROXY_ADDR => {
                let packed_semver = ProtocolVersionId::latest().into_packed_semver_with_patch(0);
                let contract = zksync_contracts::hyperchain_contract();
                let expected_input = contract
                    .function("getProtocolVersion")
                    .unwrap()
                    .encode_input(&[])
                    .unwrap();
                assert_eq!(call.data, Some(expected_input.into()));

                ethabi::Token::Uint(packed_semver)
            }
            Some(addr) if with_get_zk_chain && addr == L2_BRIDGEHUB_ADDRESS => {
                let contract = zksync_contracts::bridgehub_contract();
                let expected_input = contract
                    .function("getZKChain")
                    .unwrap()
                    .encode_input(&[Token::Uint(ERA_CHAIN_ID.into())])
                    .unwrap();
                assert_eq!(call.data, Some(expected_input.into()));

                ethabi::Token::Address(GATEWAY_DIAMOND_PROXY_ADDR)
            }
            _ => panic!("Received unexpected call"),
        })
        .with_chain_id(chain_id);
    mock.build()
}

fn create_mock_ethereum() -> MockSettlementLayer {
    create_mock_sl(L1_CHAIN_ID, false)
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

    fn report_inconsistent_batch(&mut self, number: L1BatchNumber, err: &anyhow::Error) {
        panic!("Error on batch #{number}: {err}");
    }
}

#[test_casing(2, COMMITMENT_MODES)]
#[test]
fn build_commit_tx_input_data_is_correct(commitment_mode: L1BatchCommitmentMode) {
    let contract = zksync_contracts::hyperchain_contract();
    let commit_function = contract.function("commitBatchesSharedBridge").unwrap();
    let batches = vec![
        create_l1_batch_with_metadata(1),
        create_l1_batch_with_metadata(2),
    ];

    let commit_tx_input_data = build_commit_tx_input_data(&batches, commitment_mode);

    for batch in &batches {
        let commit_data = ConsistencyChecker::extract_commit_data(
            &commit_tx_input_data,
            commit_function,
            batch.header.number,
            batch
                .header
                .protocol_version
                .map(|v| v.is_pre_gateway())
                .unwrap_or(true),
        )
        .unwrap();
        assert_eq!(
            commit_data,
            CommitBatchInfo::new(commitment_mode, batch, PubdataSendingMode::Calldata).into_token(),
        );
    }
}

#[test]
fn extracting_commit_data_for_boojum_batch() {
    let commit_function = &*POST_BOOJUM_COMMIT_FUNCTION;
    // Calldata taken from the commit transaction for `https://sepolia.explorer.zksync.io/batch/4470`;
    // `https://sepolia.etherscan.io/tx/0x300b9115037028b1f8aa2177abf98148c3df95c9b04f95a4e25baf4dfee7711f`
    let commit_tx_input_data = include_bytes!("commit_l1_batch_4470_testnet_sepolia.calldata");

    let commit_data = ConsistencyChecker::extract_commit_data(
        commit_tx_input_data,
        commit_function,
        L1BatchNumber(4_470),
        true,
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
            true,
        )
        .unwrap_err();
    }
}

#[test]
fn extracting_commit_data_for_multiple_batches() {
    let commit_function = &*POST_BOOJUM_COMMIT_FUNCTION;
    // Calldata taken from the commit transaction for `https://explorer.zksync.io/batch/351000`;
    // `https://etherscan.io/tx/0xbd8dfe0812df0da534eb95a2d2a4382d65a8172c0b648a147d60c1c2921227fd`
    let commit_tx_input_data = include_bytes!("commit_l1_batch_351000-351004_mainnet.calldata");

    for l1_batch in 351_000..=351_004 {
        let commit_data = ConsistencyChecker::extract_commit_data(
            commit_tx_input_data,
            commit_function,
            L1BatchNumber(l1_batch),
            true,
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
            true,
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
        true,
    )
    .unwrap();

    assert_matches!(
        commit_data,
        ethabi::Token::Tuple(tuple) if tuple[0] == ethabi::Token::Uint(200_000.into())
    );
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SaveAction<'a> {
    InsertBatch(&'a L1BatchWithMetadata, &'a L2BlockHeader),
    SaveMetadata(&'a L1BatchWithMetadata),
    InsertCommitTx(L1BatchNumber),
}

impl SaveAction<'_> {
    async fn apply(
        self,
        storage: &mut Connection<'_, Core>,
        commit_tx_hash_by_l1_batch: &HashMap<L1BatchNumber, H256>,
        chain_id_by_l1_batch: &HashMap<L1BatchNumber, SLChainId>,
    ) {
        match self {
            Self::InsertBatch(l1_batch, l2_block) => {
                storage
                    .blocks_dal()
                    .insert_mock_l1_batch(&l1_batch.header)
                    .await
                    .unwrap();
                storage
                    .blocks_dal()
                    .insert_l2_block(l2_block, l1_batch.header.number)
                    .await
                    .unwrap();

                if L1BatchCommitmentMode::from(l2_block.pubdata_params.pubdata_type)
                    == L1BatchCommitmentMode::Validium
                {
                    storage
                        .data_availability_dal()
                        .insert_l1_batch_da(
                            l1_batch.header.number,
                            "",
                            NaiveDateTime::default(),
                            PubdataType::NoDA,
                            Some(&[]),
                            Some(Address::random()),
                        )
                        .await
                        .unwrap();
                }
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
                let chain_id = chain_id_by_l1_batch.get(&l1_batch_number).copied();
                storage
                    .eth_sender_dal()
                    .insert_bogus_confirmed_eth_tx(
                        l1_batch_number,
                        AggregatedActionType::Commit,
                        commit_tx_hash,
                        chrono::Utc::now(),
                        chain_id,
                        EthTxFinalityStatus::Finalized,
                    )
                    .await
                    .unwrap();
            }
        }
    }
}

pub(crate) type SaveActionMapper =
    fn(&[(L1BatchWithMetadata, L2BlockHeader)]) -> Vec<SaveAction<'_>>;

/// Various strategies to persist L1 batches in the DB. Strings are added for debugging failed test cases.
const SAVE_ACTION_MAPPERS: [(&str, SaveActionMapper); 4] = [
    ("sequential_metadata_first", |l1_batches| {
        l1_batches
            .iter()
            .flat_map(|(batch, block)| {
                [
                    SaveAction::InsertBatch(batch, block),
                    SaveAction::SaveMetadata(batch),
                    SaveAction::InsertCommitTx(batch.header.number),
                ]
            })
            .collect()
    }),
    ("sequential_commit_txs_first", |l1_batches| {
        l1_batches
            .iter()
            .flat_map(|(batch, block)| {
                [
                    SaveAction::InsertBatch(batch, block),
                    SaveAction::InsertCommitTx(batch.header.number),
                    SaveAction::SaveMetadata(batch),
                ]
            })
            .collect()
    }),
    ("all_metadata_first", |l1_batches| {
        let commit_tx_actions = l1_batches
            .iter()
            .map(|(batch, _)| SaveAction::InsertCommitTx(batch.header.number));
        l1_batches
            .iter()
            .map(|(batch, block)| SaveAction::InsertBatch(batch, block))
            .chain(
                l1_batches
                    .iter()
                    .map(|(batch, _)| SaveAction::SaveMetadata(batch)),
            )
            .chain(commit_tx_actions)
            .collect()
    }),
    ("all_commit_txs_first", |l1_batches| {
        let commit_tx_actions = l1_batches
            .iter()
            .map(|(batch, _)| SaveAction::InsertCommitTx(batch.header.number));
        l1_batches
            .iter()
            .map(|(batch, block)| SaveAction::InsertBatch(batch, block))
            .chain(commit_tx_actions)
            .chain(
                l1_batches
                    .iter()
                    .map(|(batch, _)| SaveAction::SaveMetadata(batch)),
            )
            .collect()
    }),
];

fn l1_batch_commit_log(l1_batch: &L1BatchWithMetadata) -> Log {
    static BLOCK_COMMIT_EVENT_HASH: Lazy<H256> = Lazy::new(|| {
        zksync_contracts::hyperchain_contract()
            .event("BlockCommit")
            .unwrap()
            .signature()
    });

    Log {
        address: L1_DIAMOND_PROXY_ADDR,
        topics: vec![
            *BLOCK_COMMIT_EVENT_HASH,
            H256::from_low_u64_be(l1_batch.header.number.0.into()), // batch number
            l1_batch.metadata.root_hash,                            // batch hash
            l1_batch.metadata.commitment,                           // commitment
        ],
        data: vec![].into(),
        block_hash: None,
        block_number: None,
        transaction_hash: None,
        transaction_index: None,
        log_index: None,
        transaction_log_index: None,
        log_type: Some("mined".into()),
        removed: None,
        block_timestamp: None,
    }
}

#[test_casing(24, Product(([10, 3, 1], SAVE_ACTION_MAPPERS, COMMITMENT_MODES)))]
#[tokio::test]
async fn normal_checker_function(
    batches_per_transaction: usize,
    (mapper_name, save_actions_mapper): (&'static str, SaveActionMapper),
    commitment_mode: L1BatchCommitmentMode,
) {
    println!("Using save_actions_mapper={mapper_name}");

    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let l1_batches: Vec<_> = (1..=10).map(create_l1_batch_with_metadata).collect();
    let mut commit_tx_hash_by_l1_batch = HashMap::with_capacity(l1_batches.len());
    let client = create_mock_ethereum();

    for (i, l1_batches) in l1_batches.chunks(batches_per_transaction).enumerate() {
        let input_data = build_commit_tx_input_data(l1_batches, commitment_mode);
        let signed_tx = client.sign_prepared_tx(
            input_data.clone(),
            VALIDATOR_TIMELOCK_ADDR,
            Options {
                nonce: Some(i.into()),
                ..Options::default()
            },
        );
        let signed_tx = signed_tx.unwrap();
        client.as_ref().send_raw_tx(signed_tx.raw_tx).await.unwrap();
        client
            .execute_tx(signed_tx.hash, true, EthTxFinalityStatus::Finalized)
            .with_logs(l1_batches.iter().map(l1_batch_commit_log).collect());

        commit_tx_hash_by_l1_batch.extend(
            l1_batches
                .iter()
                .map(|batch| (batch.header.number, signed_tx.hash)),
        );
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        event_handler: Box::new(l1_batch_updates_sender),
        ..create_mock_checker(client, pool.clone()).await
    };

    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    let batches_with_blocks = l1_batches
        .iter()
        .map(|batch| {
            let mut l2_block = create_l2_block(batch_to_block_number(batch));
            if commitment_mode == L1BatchCommitmentMode::Validium {
                l2_block.pubdata_params.pubdata_type = PubdataType::NoDA;
            }
            (batch.to_owned(), l2_block)
        })
        .collect::<Vec<_>>();

    // Add new batches to the storage.
    for save_action in save_actions_mapper(&batches_with_blocks) {
        save_action
            .apply(
                &mut storage,
                &commit_tx_hash_by_l1_batch,
                &Default::default(),
            )
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

    // Send a stop request to the checker and wait for it to stop.
    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

#[test_casing(8, Product((SAVE_ACTION_MAPPERS, COMMITMENT_MODES)))]
#[tokio::test]
async fn checker_processes_pre_boojum_batches(
    (mapper_name, save_actions_mapper): (&'static str, SaveActionMapper),
    commitment_mode: L1BatchCommitmentMode,
) {
    println!("Using save_actions_mapper={mapper_name}");

    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::load_genesis_params(GenesisConfig {
        protocol_version: Some(ProtocolSemanticVersion {
            minor: PRE_BOOJUM_PROTOCOL_VERSION,
            patch: 0.into(),
        }),
        ..mock_genesis_config()
    })
    .unwrap();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    let l1_batches: Vec<_> = (1..=5)
        .map(create_pre_boojum_l1_batch_with_metadata)
        .chain((6..=10).map(create_l1_batch_with_metadata))
        .collect();
    let mut commit_tx_hash_by_l1_batch = HashMap::with_capacity(l1_batches.len());
    let client = create_mock_ethereum();

    for (i, l1_batch) in l1_batches.iter().enumerate() {
        let input_data = build_commit_tx_input_data(slice::from_ref(l1_batch), commitment_mode);
        let signed_tx = client.sign_prepared_tx(
            input_data.clone(),
            VALIDATOR_TIMELOCK_ADDR,
            Options {
                nonce: Some(i.into()),
                ..Options::default()
            },
        );
        let signed_tx = signed_tx.unwrap();
        client.as_ref().send_raw_tx(signed_tx.raw_tx).await.unwrap();
        client
            .execute_tx(signed_tx.hash, true, EthTxFinalityStatus::Finalized)
            .with_logs(vec![l1_batch_commit_log(l1_batch)]);

        commit_tx_hash_by_l1_batch.insert(l1_batch.header.number, signed_tx.hash);
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        event_handler: Box::new(l1_batch_updates_sender),
        ..create_mock_checker(client, pool.clone()).await
    };

    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    let batches_with_blocks = l1_batches
        .iter()
        .map(|batch| {
            let mut l2_block = create_l2_block(batch_to_block_number(batch));
            if commitment_mode == L1BatchCommitmentMode::Validium {
                l2_block.pubdata_params.pubdata_type = PubdataType::NoDA;
            }
            (batch.to_owned(), l2_block)
        })
        .collect::<Vec<_>>();

    // Add new batches to the storage.
    for save_action in save_actions_mapper(&batches_with_blocks) {
        save_action
            .apply(
                &mut storage,
                &commit_tx_hash_by_l1_batch,
                &Default::default(),
            )
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

    // Send a stop request to the checker and wait for it to stop.
    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

#[test_casing(4, Product(([false, true], COMMITMENT_MODES)))]
#[tokio::test]
async fn checker_functions_after_snapshot_recovery(
    delay_batch_insertion: bool,
    commitment_mode: L1BatchCommitmentMode,
) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    let l1_batch = create_l1_batch_with_metadata(99);
    let mut l2_block = create_l2_block(batch_to_block_number(&l1_batch));
    if commitment_mode == L1BatchCommitmentMode::Validium {
        l2_block.pubdata_params.pubdata_type = PubdataType::NoDA;
    }

    let commit_tx_input_data =
        build_commit_tx_input_data(slice::from_ref(&l1_batch), commitment_mode);
    let client = create_mock_ethereum();
    let signed_tx = client.sign_prepared_tx(
        commit_tx_input_data.clone(),
        VALIDATOR_TIMELOCK_ADDR,
        Options {
            nonce: Some(0.into()),
            ..Options::default()
        },
    );
    let signed_tx = signed_tx.unwrap();
    let commit_tx_hash = signed_tx.hash;
    client.as_ref().send_raw_tx(signed_tx.raw_tx).await.unwrap();
    client
        .execute_tx(commit_tx_hash, true, EthTxFinalityStatus::Finalized)
        .with_logs(vec![l1_batch_commit_log(&l1_batch)]);

    let save_actions = [
        SaveAction::InsertBatch(&l1_batch, &l2_block),
        SaveAction::SaveMetadata(&l1_batch),
        SaveAction::InsertCommitTx(l1_batch.header.number),
    ];
    let commit_tx_hash_by_l1_batch = HashMap::from([(l1_batch.header.number, commit_tx_hash)]);

    if !delay_batch_insertion {
        for &save_action in &save_actions {
            save_action
                .apply(
                    &mut storage,
                    &commit_tx_hash_by_l1_batch,
                    &Default::default(),
                )
                .await;
        }
    }

    let (l1_batch_updates_sender, mut l1_batch_updates_receiver) = mpsc::unbounded_channel();
    let checker = ConsistencyChecker {
        event_handler: Box::new(l1_batch_updates_sender),
        ..create_mock_checker(client, pool.clone()).await
    };
    let (stop_sender, stop_receiver) = watch::channel(false);
    let checker_task = tokio::spawn(checker.run(stop_receiver));

    if delay_batch_insertion {
        tokio::time::sleep(Duration::from_millis(10)).await;
        for &save_action in &save_actions {
            save_action
                .apply(
                    &mut storage,
                    &commit_tx_hash_by_l1_batch,
                    &Default::default(),
                )
                .await;
        }
    }

    // Wait until the batch is checked.
    let checked_batch = l1_batch_updates_receiver.recv().await.unwrap();
    assert_eq!(checked_batch, l1_batch.header.number);
    let last_reported_batch = storage
        .blocks_dal()
        .get_consistency_checker_last_processed_l1_batch()
        .await
        .unwrap();
    assert_eq!(last_reported_batch, l1_batch.header.number);

    stop_sender.send_replace(true);
    checker_task.await.unwrap().unwrap();
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum IncorrectDataKind {
    MissingStatus,
    MismatchedStatus,
    NoCommitLog,
    BogusCommitLogOrigin,
    BogusSoliditySelector,
    BogusCommitDataFormat,
    MismatchedCommitDataTimestamp,
    CommitDataForAnotherBatch,
    CommitDataForPreBoojum,
}

impl IncorrectDataKind {
    const ALL: [Self; 9] = [
        Self::MissingStatus,
        Self::MismatchedStatus,
        Self::NoCommitLog,
        Self::BogusCommitLogOrigin,
        Self::BogusSoliditySelector,
        Self::BogusCommitDataFormat,
        Self::MismatchedCommitDataTimestamp,
        Self::CommitDataForAnotherBatch,
        Self::CommitDataForPreBoojum,
    ];

    async fn apply(
        self,
        client: &MockSettlementLayer,
        l1_batch: &L1BatchWithMetadata,
        commitment_mode: L1BatchCommitmentMode,
    ) -> H256 {
        let mut log_origin = Some(L1_DIAMOND_PROXY_ADDR);
        let (commit_tx_input_data, successful_status) = match self {
            Self::MissingStatus => {
                return H256::zero(); // Do not execute the transaction
            }
            Self::MismatchedStatus => {
                let commit_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(l1_batch), commitment_mode);
                (commit_tx_input_data, false)
            }
            Self::NoCommitLog => {
                log_origin = None;
                let commit_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(l1_batch), commitment_mode);
                (commit_tx_input_data, true)
            }
            Self::BogusCommitLogOrigin => {
                log_origin = Some(VALIDATOR_TIMELOCK_ADDR);
                let commit_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(l1_batch), commitment_mode);
                (commit_tx_input_data, true)
            }
            Self::BogusSoliditySelector => {
                let mut commit_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(l1_batch), commitment_mode);
                commit_tx_input_data[..4].copy_from_slice(b"test");
                (commit_tx_input_data, true)
            }
            Self::BogusCommitDataFormat => {
                let commit_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(l1_batch), commitment_mode);
                let mut bogus_tx_input_data = commit_tx_input_data[..4].to_vec(); // Preserve the function selector
                bogus_tx_input_data
                    .extend_from_slice(&ethabi::encode(&[ethabi::Token::Bool(true)]));
                (bogus_tx_input_data, true)
            }
            Self::MismatchedCommitDataTimestamp => {
                let mut l1_batch = create_l1_batch_with_metadata(1);
                l1_batch.header.timestamp += 1;
                let bogus_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(&l1_batch), commitment_mode);
                (bogus_tx_input_data, true)
            }
            Self::CommitDataForAnotherBatch => {
                let l1_batch = create_l1_batch_with_metadata(100);
                let bogus_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(&l1_batch), commitment_mode);
                (bogus_tx_input_data, true)
            }
            Self::CommitDataForPreBoojum => {
                let mut l1_batch = create_l1_batch_with_metadata(1);
                l1_batch.header.protocol_version = Some(ProtocolVersionId::Version0);
                let bogus_tx_input_data =
                    build_commit_tx_input_data(slice::from_ref(&l1_batch), commitment_mode);
                (bogus_tx_input_data, true)
            }
        };

        let signed_tx = client.sign_prepared_tx(
            commit_tx_input_data,
            VALIDATOR_TIMELOCK_ADDR,
            Options {
                nonce: Some(0.into()),
                ..Options::default()
            },
        );
        let signed_tx = signed_tx.unwrap();
        let tx_logs = if let Some(address) = log_origin {
            vec![Log {
                address,
                ..l1_batch_commit_log(l1_batch)
            }]
        } else {
            vec![]
        };
        client.as_ref().send_raw_tx(signed_tx.raw_tx).await.unwrap();
        client
            .execute_tx(
                signed_tx.hash,
                successful_status,
                EthTxFinalityStatus::Finalized,
            )
            .with_logs(tx_logs);
        signed_tx.hash
    }
}

#[test_casing(18, Product((IncorrectDataKind::ALL, [false], COMMITMENT_MODES)))]
// ^ `snapshot_recovery = true` is tested below; we don't want to run it with all incorrect data kinds
#[tokio::test]
async fn checker_detects_incorrect_tx_data(
    kind: IncorrectDataKind,
    snapshot_recovery: bool,
    commitment_mode: L1BatchCommitmentMode,
) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    if snapshot_recovery {
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
    } else {
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
    }

    let l1_batch = create_l1_batch_with_metadata(if snapshot_recovery { 99 } else { 1 });
    let mut l2_block = create_l2_block(batch_to_block_number(&l1_batch));
    if commitment_mode == L1BatchCommitmentMode::Validium {
        l2_block.pubdata_params.pubdata_type = PubdataType::NoDA;
    }

    let client = create_mock_ethereum();
    let commit_tx_hash = kind.apply(&client, &l1_batch, commitment_mode).await;
    let commit_tx_hash_by_l1_batch = HashMap::from([(l1_batch.header.number, commit_tx_hash)]);

    let save_actions = [
        SaveAction::InsertBatch(&l1_batch, &l2_block),
        SaveAction::SaveMetadata(&l1_batch),
        SaveAction::InsertCommitTx(l1_batch.header.number),
    ];
    for save_action in save_actions {
        save_action
            .apply(
                &mut storage,
                &commit_tx_hash_by_l1_batch,
                &Default::default(),
            )
            .await;
    }
    drop(storage);

    let checker = create_mock_checker(client, pool).await;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    // The checker must stop with an error.
    tokio::time::timeout(Duration::from_secs(30), checker.run(stop_receiver))
        .await
        .expect("Timed out waiting for checker to stop")
        .unwrap_err();
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn checker_detects_incorrect_tx_data_after_snapshot_recovery(
    commitment_mode: L1BatchCommitmentMode,
) {
    checker_detects_incorrect_tx_data(
        IncorrectDataKind::CommitDataForAnotherBatch,
        true,
        commitment_mode,
    )
    .await;
}
