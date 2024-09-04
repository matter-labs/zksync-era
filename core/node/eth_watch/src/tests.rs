use std::{collections::HashMap, convert::TryInto, sync::Arc};

use tokio::sync::RwLock;
use zksync_contracts::{chain_admin_contract, governance_contract, hyperchain_contract};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{ContractCallError, EnrichedClientResult};
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_types::{
    abi, ethabi,
    ethabi::Token,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    protocol_upgrade::{ProtocolUpgradeTx, ProtocolUpgradeTxCommonData},
    protocol_version::ProtocolSemanticVersion,
    web3::{BlockNumber, Log},
    Address, Execute, L1TxCommonData, PriorityOpId, ProtocolUpgrade, ProtocolVersion,
    ProtocolVersionId, SLChainId, Transaction, H256, U256,
};

use crate::{client::EthClient, EthWatch};

#[derive(Debug)]
struct FakeEthClientData {
    transactions: HashMap<u64, Vec<Log>>,
    diamond_upgrades: HashMap<u64, Vec<Log>>,
    governance_upgrades: HashMap<u64, Vec<Log>>,
    last_finalized_block_number: u64,
    chain_id: SLChainId,
    processed_priority_transactions_count: u64,
}

impl FakeEthClientData {
    fn new(chain_id: SLChainId) -> Self {
        Self {
            transactions: Default::default(),
            diamond_upgrades: Default::default(),
            governance_upgrades: Default::default(),
            last_finalized_block_number: 0,
            chain_id,
            processed_priority_transactions_count: 0,
        }
    }

    fn add_transactions(&mut self, transactions: &[L1Tx]) {
        for transaction in transactions {
            let eth_block = transaction.eth_block();
            self.transactions
                .entry(eth_block.0 as u64)
                .or_default()
                .push(tx_into_log(transaction.clone()));
            self.processed_priority_transactions_count += 1;
        }
    }

    fn add_governance_upgrades(&mut self, upgrades: &[(ProtocolUpgrade, u64)]) {
        for (upgrade, eth_block) in upgrades {
            self.governance_upgrades
                .entry(*eth_block)
                .or_default()
                .push(upgrade_into_governor_log(upgrade.clone(), *eth_block));
        }
    }

    fn set_last_finalized_block_number(&mut self, number: u64) {
        self.last_finalized_block_number = number;
    }

    fn set_processed_priority_transactions_count(&mut self, number: u64) {
        self.processed_priority_transactions_count = number;
    }
}

#[derive(Debug, Clone)]
struct MockEthClient {
    inner: Arc<RwLock<FakeEthClientData>>,
}

impl MockEthClient {
    fn new(chain_id: SLChainId) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FakeEthClientData::new(chain_id))),
        }
    }

    async fn add_transactions(&mut self, transactions: &[L1Tx]) {
        self.inner.write().await.add_transactions(transactions);
    }

    async fn add_governance_upgrades(&mut self, upgrades: &[(ProtocolUpgrade, u64)]) {
        self.inner.write().await.add_governance_upgrades(upgrades);
    }

    async fn set_last_finalized_block_number(&mut self, number: u64) {
        self.inner
            .write()
            .await
            .set_last_finalized_block_number(number);
    }

    async fn set_processed_priority_transactions_count(&mut self, number: u64) {
        self.inner
            .write()
            .await
            .set_processed_priority_transactions_count(number)
    }

    async fn block_to_number(&self, block: BlockNumber) -> u64 {
        match block {
            BlockNumber::Earliest => 0,
            BlockNumber::Number(number) => number.as_u64(),
            BlockNumber::Pending
            | BlockNumber::Latest
            | BlockNumber::Finalized
            | BlockNumber::Safe => unreachable!(),
        }
    }
}

#[async_trait::async_trait]
impl EthClient for MockEthClient {
    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topic: H256,
        _retries_left: usize,
    ) -> EnrichedClientResult<Vec<Log>> {
        let from = self.block_to_number(from).await;
        let to = self.block_to_number(to).await;
        let mut logs = vec![];
        for number in from..=to {
            if let Some(ops) = self.inner.read().await.transactions.get(&number) {
                logs.extend_from_slice(ops);
            }
            if let Some(ops) = self.inner.read().await.diamond_upgrades.get(&number) {
                logs.extend_from_slice(ops);
            }
            if let Some(ops) = self.inner.read().await.governance_upgrades.get(&number) {
                logs.extend_from_slice(ops);
            }
        }
        Ok(logs
            .into_iter()
            .filter(|log| log.topics.contains(&topic))
            .collect())
    }

    async fn scheduler_vk_hash(
        &self,
        _verifier_address: Address,
    ) -> Result<H256, ContractCallError> {
        Ok(H256::zero())
    }

    async fn finalized_block_number(&self) -> EnrichedClientResult<u64> {
        Ok(self.inner.read().await.last_finalized_block_number)
    }

    async fn diamond_cut_by_version(
        &self,
        _packed_version: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        unimplemented!()
    }

    async fn get_total_priority_txs(&self) -> Result<u64, ContractCallError> {
        Ok(self
            .inner
            .read()
            .await
            .processed_priority_transactions_count)
    }

    async fn chain_id(&self) -> EnrichedClientResult<SLChainId> {
        Ok(self.inner.read().await.chain_id)
    }
}

fn build_l1_tx(serial_id: u64, eth_block: u64) -> L1Tx {
    let tx = L1Tx {
        execute: Execute {
            contract_address: Address::repeat_byte(0x11),
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
    let tx =
        Transaction::try_from(abi::Transaction::try_from(Transaction::from(tx)).unwrap()).unwrap();
    tx.try_into().unwrap()
}

fn build_upgrade_tx(id: ProtocolVersionId, eth_block: u64) -> ProtocolUpgradeTx {
    let tx = ProtocolUpgradeTx {
        execute: Execute {
            contract_address: Address::repeat_byte(0x11),
            calldata: vec![1, 2, 3],
            factory_deps: vec![],
            value: U256::zero(),
        },
        common_data: ProtocolUpgradeTxCommonData {
            upgrade_id: id,
            sender: [1u8; 20].into(),
            eth_block,
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
    Transaction::try_from(abi::Transaction::try_from(Transaction::from(tx)).unwrap())
        .unwrap()
        .try_into()
        .unwrap()
}

async fn create_test_watcher(
    connection_pool: ConnectionPool<Core>,
    is_gateway: bool,
) -> (EthWatch, MockEthClient, MockEthClient) {
    let l1_client = MockEthClient::new(SLChainId(42));
    let sl_client = if is_gateway {
        MockEthClient::new(SLChainId(123))
    } else {
        l1_client.clone()
    };
    let watcher = EthWatch::new(
        Address::default(),
        &governance_contract(),
        &chain_admin_contract(),
        Box::new(l1_client.clone()),
        Box::new(sl_client.clone()),
        connection_pool,
        std::time::Duration::from_nanos(1),
        SyncMerkleTree::from_hashes(std::iter::empty(), None),
    )
    .await
    .unwrap();

    (watcher, l1_client, sl_client)
}

async fn create_l1_test_watcher(
    connection_pool: ConnectionPool<Core>,
) -> (EthWatch, MockEthClient) {
    let (watcher, l1_client, _) = create_test_watcher(connection_pool, false).await;
    (watcher, l1_client)
}

async fn create_gateway_test_watcher(
    connection_pool: ConnectionPool<Core>,
) -> (EthWatch, MockEthClient, MockEthClient) {
    create_test_watcher(connection_pool, true).await
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
async fn test_gap_in_governance_upgrades() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;
    let (mut watcher, mut client) = create_l1_test_watcher(connection_pool.clone()).await;

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_governance_upgrades(&[(
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
async fn test_normal_operation_governance_upgrades() {
    zksync_concurrency::testonly::abort_on_panic();
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = MockEthClient::new(SLChainId(42));
    let mut watcher = EthWatch::new(
        Address::default(),
        &governance_contract(),
        &chain_admin_contract(),
        Box::new(client.clone()),
        Box::new(client.clone()),
        connection_pool.clone(),
        std::time::Duration::from_nanos(1),
        SyncMerkleTree::from_hashes(std::iter::empty(), None),
    )
    .await
    .unwrap();

    let mut storage = connection_pool.connection().await.unwrap();
    client
        .add_governance_upgrades(&[
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
                    tx: Some(build_upgrade_tx(ProtocolVersionId::next(), 18)),
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
    assert_eq!(tx.common_data.upgrade_id, ProtocolVersionId::next());
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

async fn get_all_db_txs(storage: &mut Connection<'_, Core>) -> Vec<Transaction> {
    storage.transactions_dal().reset_mempool().await.unwrap();
    storage
        .transactions_dal()
        .sync_mempool(&[], &[], 0, 0, 1000)
        .await
        .unwrap()
}

fn tx_into_log(tx: L1Tx) -> Log {
    let tx = abi::Transaction::try_from(Transaction::from(tx)).unwrap();
    let abi::Transaction::L1 {
        tx,
        factory_deps,
        eth_block,
        ..
    } = tx
    else {
        unreachable!()
    };

    let data = ethabi::encode(
        &abi::NewPriorityRequest {
            tx_id: tx.nonce,
            tx_hash: tx.hash().into(),
            expiration_timestamp: u64::MAX,
            transaction: tx,
            factory_deps,
        }
        .encode(),
    );

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![hyperchain_contract()
            .event("NewPriorityRequest")
            .expect("NewPriorityRequest event is missing in abi")
            .signature()],
        data: data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(eth_block.into()),
        transaction_hash: Some(H256::default()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
        block_timestamp: None,
    }
}

fn upgrade_into_governor_log(upgrade: ProtocolUpgrade, eth_block: u64) -> Log {
    let diamond_cut = upgrade_into_diamond_cut(upgrade);
    let execute_upgrade_selector = hyperchain_contract()
        .function("executeUpgrade")
        .unwrap()
        .short_signature();
    let diamond_upgrade_calldata = execute_upgrade_selector
        .iter()
        .copied()
        .chain(ethabi::encode(&[diamond_cut]))
        .collect();
    let governance_call = Token::Tuple(vec![
        Token::Address(Default::default()),
        Token::Uint(U256::default()),
        Token::Bytes(diamond_upgrade_calldata),
    ]);
    let governance_operation = Token::Tuple(vec![
        Token::Array(vec![governance_call]),
        Token::FixedBytes(vec![0u8; 32]),
        Token::FixedBytes(vec![0u8; 32]),
    ]);
    let final_data = ethabi::encode(&[Token::FixedBytes(vec![0u8; 32]), governance_operation]);

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            governance_contract()
                .event("TransparentOperationScheduled")
                .expect("TransparentOperationScheduled event is missing in abi")
                .signature(),
            Default::default(),
        ],
        data: final_data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(eth_block.into()),
        transaction_hash: Some(H256::random()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
        block_timestamp: None,
    }
}

fn upgrade_into_diamond_cut(upgrade: ProtocolUpgrade) -> Token {
    let abi::Transaction::L1 {
        tx, factory_deps, ..
    } = upgrade
        .tx
        .map(|tx| Transaction::from(tx).try_into().unwrap())
        .unwrap_or(abi::Transaction::L1 {
            tx: Default::default(),
            factory_deps: vec![],
            eth_block: 0,
        })
    else {
        unreachable!()
    };
    let upgrade_token = abi::ProposedUpgrade {
        l2_protocol_upgrade_tx: tx,
        factory_deps,
        bootloader_hash: upgrade.bootloader_code_hash.unwrap_or_default().into(),
        default_account_hash: upgrade.default_account_code_hash.unwrap_or_default().into(),
        verifier: upgrade.verifier_address.unwrap_or_default(),
        verifier_params: upgrade.verifier_params.unwrap_or_default().into(),
        l1_contracts_upgrade_calldata: vec![],
        post_upgrade_calldata: vec![],
        upgrade_timestamp: upgrade.timestamp.into(),
        new_protocol_version: upgrade.version.pack(),
    }
    .encode();
    Token::Tuple(vec![
        Token::Array(vec![]),
        Token::Address(Default::default()),
        Token::Bytes(
            vec![0u8; 4]
                .into_iter()
                .chain(ethabi::encode(&[upgrade_token]))
                .collect(),
        ),
    ])
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
