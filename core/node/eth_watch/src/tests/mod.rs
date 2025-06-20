use std::convert::TryInto;

use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{
    abi,
    aggregated_operations::AggregatedActionType,
    api::ChainAggProof,
    block::L1BatchHeader,
    commitment::L1BatchCommitmentArtifacts,
    eth_sender::EthTxFinalityStatus,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    l2_to_l1_log::BatchAndChainMerklePath,
    protocol_upgrade::{ProtocolUpgradeTx, ProtocolUpgradeTxCommonData},
    protocol_version::ProtocolSemanticVersion,
    settlement::SettlementLayer,
    Address, Execute, L1BatchNumber, L1TxCommonData, L2BlockNumber, L2ChainId, PriorityOpId,
    ProtocolUpgrade, ProtocolVersion, ProtocolVersionId, SLChainId, Transaction, H256, U256,
};

use crate::{tests::client::MockEthClient, EthWatch, ZkSyncExtentionEthClient};

mod client;

const SL_CHAIN_ID: SLChainId = SLChainId(505);

fn build_l1_tx(serial_id: u64, eth_block: u64) -> L1Tx {
    let tx = L1Tx {
        execute: Execute {
            contract_address: Some(Address::repeat_byte(0x11)),
            calldata: vec![1, 2, 3],
            factory_deps: vec![],
            value: U256::zero(),
        },
        common_data: L1TxCommonData {
            serial_id: PriorityOpId(serial_id),
            sender: [1u8; 20].into(),
            eth_block,
            gas_limit: Default::default(),
            max_fee_per_gas: Default::default(),
            gas_per_pubdata_limit: 1u32.into(),
            full_fee: Default::default(),
            layer_2_tip_fee: U256::from(10u8),
            refund_recipient: Address::zero(),
            to_mint: Default::default(),
            priority_queue_type: PriorityQueueType::Deque,
            op_processing_type: OpProcessingType::Common,
            canonical_tx_hash: H256::default(),
        },
        received_timestamp_ms: 0,
    };
    // Convert to abi::Transaction and back, so that canonical_tx_hash is computed.
    let tx = Transaction::from_abi(
        abi::Transaction::try_from(Transaction::from(tx)).unwrap(),
        false,
    )
    .unwrap();
    tx.try_into().unwrap()
}

fn dummy_bytecode() -> Vec<u8> {
    vec![0u8; 32]
}

fn build_upgrade_tx(id: ProtocolVersionId) -> ProtocolUpgradeTx {
    let tx = ProtocolUpgradeTx {
        execute: Execute {
            contract_address: Some(Address::repeat_byte(0x11)),
            calldata: vec![1, 2, 3],
            factory_deps: vec![dummy_bytecode(), dummy_bytecode()],
            value: U256::zero(),
        },
        common_data: ProtocolUpgradeTxCommonData {
            upgrade_id: id,
            sender: [1u8; 20].into(),
            // Note, that the field is deprecated
            eth_block: 0,
            gas_limit: Default::default(),
            max_fee_per_gas: Default::default(),
            gas_per_pubdata_limit: 1u32.into(),
            refund_recipient: Address::zero(),
            to_mint: Default::default(),
            canonical_tx_hash: H256::zero(),
        },
        received_timestamp_ms: 0,
    };
    // Convert to abi::Transaction and back, so that canonical_tx_hash is computed.
    Transaction::from_abi(
        abi::Transaction::try_from(Transaction::from(tx)).unwrap(),
        false,
    )
    .unwrap()
    .try_into()
    .unwrap()
}

async fn create_test_watcher(
    connection_pool: ConnectionPool<Core>,
    settlement_layer: SettlementLayer,
) -> (EthWatch, MockEthClient, MockEthClient) {
    let l1_client = MockEthClient::new(SLChainId(42));
    let sl_client = MockEthClient::new(SL_CHAIN_ID);
    let sl_l2_client: Box<dyn ZkSyncExtentionEthClient> = if settlement_layer.is_gateway() {
        Box::new(sl_client.clone())
    } else {
        Box::new(l1_client.clone())
    };
    let watcher = EthWatch::new(
        Box::new(l1_client.clone()),
        sl_l2_client,
        Some(settlement_layer),
        connection_pool,
        std::time::Duration::from_nanos(1),
        L2ChainId::default(),
        50_000,
    )
    .await
    .unwrap();

    (watcher, l1_client, sl_client)
}

async fn create_l1_test_watcher(
    connection_pool: ConnectionPool<Core>,
) -> (EthWatch, MockEthClient) {
    let (watcher, l1_client, _) =
        create_test_watcher(connection_pool, SettlementLayer::L1(SL_CHAIN_ID)).await;
    (watcher, l1_client)
}

async fn create_gateway_test_watcher(
    connection_pool: ConnectionPool<Core>,
) -> (EthWatch, MockEthClient, MockEthClient) {
    create_test_watcher(connection_pool, SettlementLayer::Gateway(SL_CHAIN_ID)).await
}

#[test_log::test(tokio::test)]
async fn test_normal_operation_l1_txs() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut client) = create_l1_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_transactions(&[build_l1_tx(0, 10), build_l1_tx(1, 14), build_l1_tx(2, 18)])
        .await;
    client.set_last_finalized_block_number(15).await;
    // second tx will not be processed, as it's block is not finalized yet.
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage).await;
    let mut db_txs: Vec<L1Tx> = db_txs
        .into_iter()
        .map(|tx| tx.try_into().unwrap())
        .collect();
    db_txs.sort_by_key(|tx| tx.common_data.serial_id);
    assert_eq!(db_txs.len(), 2);
    let db_tx = db_txs[0].clone();
    assert_eq!(db_tx.common_data.serial_id.0, 0);
    let db_tx = db_txs[1].clone();
    assert_eq!(db_tx.common_data.serial_id.0, 1);

    client.set_last_finalized_block_number(20).await;
    // now the second tx will be processed
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage).await;
    let mut db_txs: Vec<L1Tx> = db_txs
        .into_iter()
        .map(|tx| tx.try_into().unwrap())
        .collect();
    db_txs.sort_by_key(|tx| tx.common_data.serial_id);
    assert_eq!(db_txs.len(), 3);
    let db_tx = db_txs[2].clone();
    assert_eq!(db_tx.common_data.serial_id.0, 2);
}

#[test_log::test(tokio::test)]
async fn test_gap_in_upgrade_timestamp() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut client) = create_l1_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_upgrade_timestamp(&[(
            ProtocolUpgrade {
                version: ProtocolSemanticVersion {
                    minor: ProtocolVersionId::next(),
                    patch: 0.into(),
                },
                tx: None,
                ..Default::default()
            },
            10,
        )])
        .await;
    client.set_last_finalized_block_number(15).await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_versions = storage.protocol_versions_dal().all_versions().await;
    // there should be genesis version and just added version
    assert_eq!(db_versions.len(), 2);

    let previous_version = (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap();
    let next_version = ProtocolVersionId::next();
    assert_eq!(db_versions[0].minor, previous_version);
    assert_eq!(db_versions[1].minor, next_version);
}

#[test_log::test(tokio::test)]
async fn test_normal_operation_upgrade_timestamp() {
    zksync_concurrency::testonly::abort_on_panic();
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = MockEthClient::new(SLChainId(42));
    let mut watcher = EthWatch::new(
        Box::new(client.clone()),
        Box::new(client.clone()),
        Some(SettlementLayer::L1(SL_CHAIN_ID)),
        connection_pool.clone(),
        std::time::Duration::from_nanos(1),
        L2ChainId::default(),
        50_000,
    )
    .await
    .unwrap();

    let expected_upgrade_tx = build_upgrade_tx(ProtocolVersionId::next());

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_upgrade_timestamp(&[
            (
                ProtocolUpgrade {
                    tx: None,
                    ..Default::default()
                },
                10,
            ),
            (
                ProtocolUpgrade {
                    version: ProtocolSemanticVersion {
                        minor: ProtocolVersionId::next(),
                        patch: 0.into(),
                    },
                    tx: Some(expected_upgrade_tx.clone()),
                    ..Default::default()
                },
                18,
            ),
            (
                ProtocolUpgrade {
                    version: ProtocolSemanticVersion {
                        minor: ProtocolVersionId::next(),
                        patch: 1.into(),
                    },
                    tx: None,
                    ..Default::default()
                },
                19,
            ),
        ])
        .await;
    client.set_last_finalized_block_number(15).await;
    // The second upgrade will not be processed, as it has less than 5 confirmations.
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_versions = storage.protocol_versions_dal().all_versions().await;
    // There should be genesis version and just added version.
    assert_eq!(db_versions.len(), 2);
    assert_eq!(db_versions[1].minor, ProtocolVersionId::latest());

    client.set_last_finalized_block_number(20).await;
    // Now the second and the third upgrades will be processed.
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_versions = storage.protocol_versions_dal().all_versions().await;
    let mut expected_version = ProtocolSemanticVersion {
        minor: ProtocolVersionId::next(),
        patch: 0.into(),
    };
    assert_eq!(db_versions.len(), 4);
    assert_eq!(db_versions[2], expected_version);
    expected_version.patch += 1;
    assert_eq!(db_versions[3], expected_version);

    // Check that tx was saved with the second upgrade.
    let tx = storage
        .protocol_versions_dal()
        .get_protocol_upgrade_tx(ProtocolVersionId::next())
        .await
        .unwrap()
        .expect("no protocol upgrade transaction");

    let ProtocolUpgradeTx {
        execute: expected_execute,
        common_data: expected_common_data,
        ..
    } = expected_upgrade_tx;

    let ProtocolUpgradeTx {
        execute,
        common_data,
        ..
    } = tx;
    assert_eq!(expected_execute, execute);
    assert_eq!(expected_common_data, common_data);
}

#[test_log::test(tokio::test)]
#[should_panic]
async fn test_gap_in_single_batch() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut client) = create_l1_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_transactions(&[
            build_l1_tx(0, 10),
            build_l1_tx(1, 14),
            build_l1_tx(2, 14),
            build_l1_tx(3, 14),
            build_l1_tx(5, 14),
        ])
        .await;
    client.set_last_finalized_block_number(15).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
}

#[test_log::test(tokio::test)]
#[should_panic]
async fn test_gap_between_batches() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut client) = create_l1_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_transactions(&[
            // this goes to the first batch
            build_l1_tx(0, 10),
            build_l1_tx(1, 14),
            build_l1_tx(2, 14),
            // this goes to the second batch
            build_l1_tx(4, 20),
            build_l1_tx(5, 22),
        ])
        .await;
    client.set_last_finalized_block_number(15).await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_txs = get_all_db_txs(&mut storage).await;
    assert_eq!(db_txs.len(), 3);
    client.set_last_finalized_block_number(25).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
}

#[test_log::test(tokio::test)]
async fn test_overlapping_batches() {
    zksync_concurrency::testonly::abort_on_panic();
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut client) = create_l1_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_transactions(&[
            // this goes to the first batch
            build_l1_tx(0, 10),
            build_l1_tx(1, 14),
            build_l1_tx(2, 14),
            // this goes to the second batch
            build_l1_tx(1, 20),
            build_l1_tx(2, 22),
            build_l1_tx(3, 23),
            build_l1_tx(4, 23),
        ])
        .await;
    client.set_last_finalized_block_number(15).await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_txs = get_all_db_txs(&mut storage).await;
    assert_eq!(db_txs.len(), 3);

    client.set_last_finalized_block_number(25).await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_txs = get_all_db_txs(&mut storage).await;
    assert_eq!(db_txs.len(), 5);
    let mut db_txs: Vec<L1Tx> = db_txs
        .into_iter()
        .map(|tx| tx.try_into().unwrap())
        .collect();
    db_txs.sort_by_key(|tx| tx.common_data.serial_id);
    let tx = db_txs[2].clone();
    assert_eq!(tx.common_data.serial_id.0, 2);
    let tx = db_txs[4].clone();
    assert_eq!(tx.common_data.serial_id.0, 4);
}

#[test_log::test(tokio::test)]
async fn test_transactions_get_gradually_processed_by_gateway() {
    zksync_concurrency::testonly::abort_on_panic();
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut l1_client, mut gateway_client) =
        create_gateway_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    l1_client
        .add_transactions(&[
            build_l1_tx(0, 10),
            build_l1_tx(1, 14),
            build_l1_tx(2, 14),
            build_l1_tx(3, 20),
            build_l1_tx(4, 22),
        ])
        .await;
    l1_client.set_last_finalized_block_number(15).await;
    gateway_client
        .set_processed_priority_transactions_count(2)
        .await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_txs = get_all_db_txs(&mut storage).await;
    assert_eq!(db_txs.len(), 2);

    l1_client.set_last_finalized_block_number(25).await;
    gateway_client
        .set_processed_priority_transactions_count(4)
        .await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_txs = get_all_db_txs(&mut storage).await;
    assert_eq!(db_txs.len(), 4);
    let mut db_txs: Vec<L1Tx> = db_txs
        .into_iter()
        .map(|tx| tx.try_into().unwrap())
        .collect();
    db_txs.sort_by_key(|tx| tx.common_data.serial_id);
    let tx = db_txs[2].clone();
    assert_eq!(tx.common_data.serial_id.0, 2);
    let tx = db_txs[3].clone();
    assert_eq!(tx.common_data.serial_id.0, 3);
}

#[test_log::test(tokio::test)]
async fn test_batch_root_processor_from_genesis() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    setup_batch_roots(&connection_pool, 0).await;
    let (mut watcher, _, mut sl_client) =
        create_gateway_test_watcher(connection_pool.clone()).await;

    let batch_roots = batch_roots();
    sl_client
        .add_batch_roots(&[
            (5, 1, batch_roots[0]),
            (9, 2, batch_roots[1]),
            (11, 3, batch_roots[2]),
        ])
        .await;
    sl_client
        .add_chain_roots(&[
            (
                1,
                H256::from_slice(
                    &hex::decode(
                        "10a2ef76e709d318b459be49f1e8d7f02d7120f2b501bc0afddd935f1a813c67",
                    )
                    .unwrap(),
                ),
            ),
            (
                2,
                H256::from_slice(
                    &hex::decode(
                        "e0c3330f674b6b2d578f958a1dbd66f164d068b0bb5a9fb077eca013976fda6f",
                    )
                    .unwrap(),
                ),
            ),
            (
                3,
                H256::from_slice(
                    &hex::decode(
                        "d22fc9a7b005fefecd33bb56cdbf70bcc23610e693cd21295f9920227c2cb1cc",
                    )
                    .unwrap(),
                ),
            ),
        ])
        .await;
    let chain_log_proofs = chain_log_proofs();
    sl_client.add_chain_log_proofs(chain_log_proofs).await;
    let chain_log_proofs_until_msg_root = chain_log_proofs_until_msg_root();
    sl_client
        .add_chain_log_proofs_until_msg_root(chain_log_proofs_until_msg_root)
        .await;

    sl_client.set_last_finalized_block_number(5).await;

    let mut connection = connection_pool.connection().await.unwrap();
    watcher.loop_iteration(&mut connection).await.unwrap();

    let proof1 = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(1))
        .await
        .unwrap()
        .unwrap();
    let proof1 = hex::encode(bincode::serialize(&proof1).unwrap());
    assert_eq!(proof1, "000000000600000000000000420000000000000030783030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303030303030303030303030303030303030303030303030303030303030303130303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307833373561356266393039636230323134336533363935636136353865303634316537333961613539306630303034646261393335373263343463646239643264");

    sl_client.set_last_finalized_block_number(11).await;
    watcher.loop_iteration(&mut connection).await.unwrap();

    let proof2 = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(2))
        .await
        .unwrap()
        .unwrap();
    let proof2 = hex::encode(bincode::serialize(&proof2).unwrap());
    assert_eq!(proof2, "0100000007000000000000004200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303031420000000000000030783130613265663736653730396433313862343539626534396631653864376630326437313230663262353031626330616664646439333566316138313363363742000000000000003078303030303030303030303030303030303030303030303030303030303030303230303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307861333738613230636132376237616533303731643162643763326164613030343639616263353765343239646436663438613833303932646237303539613138");

    let proof3 = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(3))
        .await
        .unwrap()
        .unwrap();
    let proof3 = hex::encode(bincode::serialize(&proof3).unwrap());
    assert_eq!(proof3, "02000000080000000000000042000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030324200000000000000307834363730306234643430616335633335616632633232646461323738376139316562353637623036633932346138666238616539613035623230633038633231420000000000000030786530633333333066363734623662326435373866393538613164626436366631363464303638623062623561396662303737656361303133393736666461366642000000000000003078303030303030303030303030303030303030303030303030303030303030303330303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307833373561356266393039636230323134336533363935636136353865303634316537333961613539306630303034646261393335373263343463646239643264");
}

#[test_log::test(tokio::test)]
async fn test_batch_root_processor_restart() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    setup_batch_roots(&connection_pool, 2).await;
    let (mut watcher, _, mut sl_client) =
        create_gateway_test_watcher(connection_pool.clone()).await;

    let batch_roots = batch_roots();
    sl_client
        .add_batch_roots(&[
            (11, 3, batch_roots[2]),
            (13, 4, batch_roots[3]),
            (14, 5, batch_roots[4]),
            (14, 6, batch_roots[5]),
        ])
        .await;
    sl_client
        .add_chain_roots(&[
            (
                3,
                H256::from_slice(
                    &hex::decode(
                        "d22fc9a7b005fefecd33bb56cdbf70bcc23610e693cd21295f9920227c2cb1cc",
                    )
                    .unwrap(),
                ),
            ),
            (
                4,
                H256::from_slice(
                    &hex::decode(
                        "53edc1f5ad79c5999bd578dfc135f9c51ebd7fafa4585b64f71d15b2dce1b728",
                    )
                    .unwrap(),
                ),
            ),
            (
                5,
                H256::from_slice(
                    &hex::decode(
                        "9a701e0c2319fa2b0948e4eb9b2d4f91c213e1cf45434890cd13772df1184b6d",
                    )
                    .unwrap(),
                ),
            ),
            (
                6,
                H256::from_slice(
                    &hex::decode(
                        "61b35796307159a6da8aa45448e6941e3438380582e2f3cb358db59598ae156f",
                    )
                    .unwrap(),
                ),
            ),
        ])
        .await;
    let chain_log_proofs = chain_log_proofs();
    sl_client.add_chain_log_proofs(chain_log_proofs).await;
    let chain_log_proofs_until_msg_root = chain_log_proofs_until_msg_root();
    sl_client
        .add_chain_log_proofs_until_msg_root(chain_log_proofs_until_msg_root)
        .await;

    sl_client.set_last_finalized_block_number(14).await;

    let mut connection = connection_pool.connection().await.unwrap();
    watcher.loop_iteration(&mut connection).await.unwrap();

    let proof = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(3))
        .await
        .unwrap()
        .unwrap();
    let proof = hex::encode(bincode::serialize(&proof).unwrap());
    assert_eq!(proof, "02000000080000000000000042000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030324200000000000000307834363730306234643430616335633335616632633232646461323738376139316562353637623036633932346138666238616539613035623230633038633231420000000000000030786530633333333066363734623662326435373866393538613164626436366631363464303638623062623561396662303737656361303133393736666461366642000000000000003078303030303030303030303030303030303030303030303030303030303030303330303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307833373561356266393039636230323134336533363935636136353865303634316537333961613539306630303034646261393335373263343463646239643264");

    let proof = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(4))
        .await
        .unwrap()
        .unwrap();
    let proof = hex::encode(bincode::serialize(&proof).unwrap());
    assert_eq!(proof, "02000000080000000000000042000000000000003078303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030334200000000000000307837623765373735373139343639366666393634616233353837393131373362636337663735356132656161393334653935373061636533393139383435313265420000000000000030786530633333333066363734623662326435373866393538613164626436366631363464303638623062623561396662303737656361303133393736666461366642000000000000003078303030303030303030303030303030303030303030303030303030303030303430303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307835353063313735316338653764626166633839303939326634353532333636663064643565623665343362653535353936386264616338633732656466316261");

    let proof = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(5))
        .await
        .unwrap()
        .unwrap();
    let proof = hex::encode(bincode::serialize(&proof).unwrap());
    assert_eq!(proof, "030000000900000000000000420000000000000030783030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303442000000000000003078343637303062346434306163356333356166326332326464613237383761393165623536376230366339323461386662386165396130356232306330386332314200000000000000307863633463343165646230633230333133343862323932623736386539626163316565386339326330396566386133323737633265636534303963313264383661420000000000000030783533656463316635616437396335393939626435373864666331333566396335316562643766616661343538356236346637316431356232646365316237323842000000000000003078303030303030303030303030303030303030303030303030303030303030303530303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307833373561356266393039636230323134336533363935636136353865303634316537333961613539306630303034646261393335373263343463646239643264");

    let proof = connection
        .blocks_dal()
        .get_l1_batch_chain_merkle_path(L1BatchNumber(6))
        .await
        .unwrap()
        .unwrap();
    let proof = hex::encode(bincode::serialize(&proof).unwrap());
    assert_eq!(proof, "030000000900000000000000420000000000000030783030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303542000000000000003078323465653435363834376535373364313635613832333634306632303834383139636331613865333433316562633635633865363064333435343266313637324200000000000000307863633463343165646230633230333133343862323932623736386539626163316565386339326330396566386133323737633265636534303963313264383661420000000000000030783533656463316635616437396335393939626435373864666331333566396335316562643766616661343538356236346637316431356232646365316237323842000000000000003078303030303030303030303030303030303030303030303030303030303030303630303030303030303030303030303030303030303030303030303030303030334200000000000000307830303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030316639420000000000000030783031303230303031303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303042000000000000003078303932343932386331333737613663663234633339633264343666386562396466323365383131623236646333353237653534383339366664346531373362314200000000000000307833373561356266393039636230323134336533363935636136353865303634316537333961613539306630303034646261393335373263343463646239643264");
}

async fn get_all_db_txs(storage: &mut Connection<'_, Core>) -> Vec<Transaction> {
    storage.transactions_dal().reset_mempool().await.unwrap();
    storage
        .transactions_dal()
        .sync_mempool(&[], &[], 0, 0, true, 1000)
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.0)
        .collect()
}

async fn setup_db(connection_pool: &ConnectionPool<Core>) {
    connection_pool
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion {
            version: ProtocolSemanticVersion {
                minor: (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap(),
                patch: 0.into(),
            },
            ..Default::default()
        })
        .await
        .unwrap();
}

fn batch_roots() -> Vec<H256> {
    [
        "5EEBBC173358620F7F61B69D80AFE503F76190396918EB7B27CEF4DB7C51D60A",
        "B7E66115CDAAF5FFE70B53EF0AC6D0FF7D7BEB4341FEC6352A670B805AE15935",
        "09BD2AD9C01C05F760BBEC6E59BF728566551B48C0DCBD01DB797D1C703122F8",
        "B6E530FF878093B2D0CAF87780451A8F07922570E2D820B7A8541114E0D70FB5",
        "B4F195844BA1792F3C1FB57C826B2DA60EA6EEBB90BF53F706120E49BB0486EF",
        "118F6FAC96824D4E0845F7C7DF716969378F3F2038D9E9D0FEAD1FE01BA11A93",
    ]
    .into_iter()
    .map(|s| H256::from_slice(&hex::decode(s).unwrap()))
    .collect()
}

fn chain_log_proofs() -> Vec<(L1BatchNumber, ChainAggProof)> {
    vec![
        (
            L1BatchNumber(1),
            ChainAggProof {
                chain_id_leaf_proof: vec![
                    H256::from_slice(
                        &hex::decode(
                            "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                        )
                        .unwrap(),
                    ),
                    H256::from_slice(
                        &hex::decode(
                            "375a5bf909cb02143e3695ca658e0641e739aa590f0004dba93572c44cdb9d2d",
                        )
                        .unwrap(),
                    ),
                ],
                chain_id_leaf_proof_mask: 3,
            },
        ),
        (
            L1BatchNumber(2),
            ChainAggProof {
                chain_id_leaf_proof: vec![
                    H256::from_slice(
                        &hex::decode(
                            "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                        )
                        .unwrap(),
                    ),
                    H256::from_slice(
                        &hex::decode(
                            "a378a20ca27b7ae3071d1bd7c2ada00469abc57e429dd6f48a83092db7059a18",
                        )
                        .unwrap(),
                    ),
                ],
                chain_id_leaf_proof_mask: 3,
            },
        ),
        (
            L1BatchNumber(3),
            ChainAggProof {
                chain_id_leaf_proof: vec![
                    H256::from_slice(
                        &hex::decode(
                            "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                        )
                        .unwrap(),
                    ),
                    H256::from_slice(
                        &hex::decode(
                            "375a5bf909cb02143e3695ca658e0641e739aa590f0004dba93572c44cdb9d2d",
                        )
                        .unwrap(),
                    ),
                ],
                chain_id_leaf_proof_mask: 3,
            },
        ),
        (
            L1BatchNumber(4),
            ChainAggProof {
                chain_id_leaf_proof: vec![
                    H256::from_slice(
                        &hex::decode(
                            "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                        )
                        .unwrap(),
                    ),
                    H256::from_slice(
                        &hex::decode(
                            "550c1751c8e7dbafc890992f4552366f0dd5eb6e43be555968bdac8c72edf1ba",
                        )
                        .unwrap(),
                    ),
                ],
                chain_id_leaf_proof_mask: 3,
            },
        ),
        (
            L1BatchNumber(5),
            ChainAggProof {
                chain_id_leaf_proof: vec![
                    H256::from_slice(
                        &hex::decode(
                            "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                        )
                        .unwrap(),
                    ),
                    H256::from_slice(
                        &hex::decode(
                            "375a5bf909cb02143e3695ca658e0641e739aa590f0004dba93572c44cdb9d2d",
                        )
                        .unwrap(),
                    ),
                ],
                chain_id_leaf_proof_mask: 3,
            },
        ),
        (
            L1BatchNumber(6),
            ChainAggProof {
                chain_id_leaf_proof: vec![
                    H256::from_slice(
                        &hex::decode(
                            "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                        )
                        .unwrap(),
                    ),
                    H256::from_slice(
                        &hex::decode(
                            "375a5bf909cb02143e3695ca658e0641e739aa590f0004dba93572c44cdb9d2d",
                        )
                        .unwrap(),
                    ),
                ],
                chain_id_leaf_proof_mask: 3,
            },
        ),
    ]
}

fn chain_log_proofs_until_msg_root() -> Vec<(L2BlockNumber, ChainAggProof)> {
    vec![
        (
            L2BlockNumber(5),
            ChainAggProof {
                chain_id_leaf_proof: vec![H256::from_slice(
                    &hex::decode(
                        "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                    )
                    .unwrap(),
                )],
                chain_id_leaf_proof_mask: 1,
            },
        ),
        (
            L2BlockNumber(9),
            ChainAggProof {
                chain_id_leaf_proof: vec![H256::from_slice(
                    &hex::decode(
                        "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                    )
                    .unwrap(),
                )],
                chain_id_leaf_proof_mask: 1,
            },
        ),
        (
            L2BlockNumber(11),
            ChainAggProof {
                chain_id_leaf_proof: vec![H256::from_slice(
                    &hex::decode(
                        "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                    )
                    .unwrap(),
                )],
                chain_id_leaf_proof_mask: 1,
            },
        ),
        (
            L2BlockNumber(13),
            ChainAggProof {
                chain_id_leaf_proof: vec![H256::from_slice(
                    &hex::decode(
                        "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                    )
                    .unwrap(),
                )],
                chain_id_leaf_proof_mask: 1,
            },
        ),
        (
            L2BlockNumber(14),
            ChainAggProof {
                chain_id_leaf_proof: vec![H256::from_slice(
                    &hex::decode(
                        "0924928c1377a6cf24c39c2d46f8eb9df23e811b26dc3527e548396fd4e173b1",
                    )
                    .unwrap(),
                )],
                chain_id_leaf_proof_mask: 1,
            },
        ),
    ]
}

async fn setup_batch_roots(
    connection_pool: &ConnectionPool<Core>,
    number_of_processed_batches: usize,
) {
    let batch_roots = batch_roots();

    let mut connection = connection_pool.connection().await.unwrap();

    assert!(number_of_processed_batches <= batch_roots.len());
    for (i, root) in batch_roots.into_iter().enumerate() {
        let batch_number = L1BatchNumber(i as u32 + 1);
        let header = L1BatchHeader::new(
            batch_number,
            i as u64,
            Default::default(),
            (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap(),
        );
        connection
            .blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
        connection
            .blocks_dal()
            .save_l1_batch_commitment_artifacts(
                batch_number,
                &L1BatchCommitmentArtifacts {
                    l2_l1_merkle_root: root,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let eth_tx_id = connection
            .eth_sender_dal()
            .save_eth_tx(
                i as u64,
                Default::default(),
                AggregatedActionType::Execute,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                true,
            )
            .await
            .unwrap()
            .id;
        connection
            .eth_sender_dal()
            .set_chain_id(eth_tx_id, SL_CHAIN_ID.0)
            .await
            .unwrap();
        connection
            .blocks_dal()
            .set_eth_tx_id(
                batch_number..=batch_number,
                eth_tx_id,
                AggregatedActionType::Execute,
            )
            .await
            .unwrap();
        let tx_hash = H256::random();
        connection
            .eth_sender_dal()
            .insert_tx_history(eth_tx_id, 0, 0, None, None, tx_hash, &[], 0, None)
            .await
            .unwrap();
        connection
            .eth_sender_dal()
            .confirm_tx(tx_hash, EthTxFinalityStatus::Finalized, U256::zero())
            .await
            .unwrap();

        if i < number_of_processed_batches {
            connection
                .blocks_dal()
                .set_batch_chain_merkle_path(
                    batch_number,
                    BatchAndChainMerklePath {
                        batch_proof_len: 0,
                        proof: Vec::new(),
                    },
                )
                .await
                .unwrap();
            connection
                .blocks_dal()
                .set_batch_chain_local_merkle_path(
                    batch_number,
                    BatchAndChainMerklePath {
                        batch_proof_len: 0,
                        proof: Vec::new(),
                    },
                )
                .await
                .unwrap();
        }
    }
}
