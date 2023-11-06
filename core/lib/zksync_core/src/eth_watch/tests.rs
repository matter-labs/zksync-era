use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use tokio::sync::RwLock;

use zksync_contracts::{governance_contract, zksync_contract};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::protocol_version::{ProtocolUpgradeTx, ProtocolUpgradeTxCommonData};
use zksync_types::web3::types::{Address, BlockNumber};
use zksync_types::{
    ethabi::{encode, Hash, Token},
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    web3::types::Log,
    Execute, L1TxCommonData, PriorityOpId, ProtocolUpgrade, ProtocolVersion, ProtocolVersionId,
    Transaction, H256, U256,
};

use super::client::Error;
use crate::eth_watch::{
    client::EthClient, event_processors::upgrades::UPGRADE_PROPOSAL_SIGNATURE, EthWatch,
};

struct FakeEthClientData {
    transactions: HashMap<u64, Vec<Log>>,
    diamond_upgrades: HashMap<u64, Vec<Log>>,
    governance_upgrades: HashMap<u64, Vec<Log>>,
    last_finalized_block_number: u64,
}

impl FakeEthClientData {
    fn new() -> Self {
        Self {
            transactions: Default::default(),
            diamond_upgrades: Default::default(),
            governance_upgrades: Default::default(),
            last_finalized_block_number: 0,
        }
    }

    fn add_transactions(&mut self, transactions: &[L1Tx]) {
        for transaction in transactions {
            let eth_block = transaction.eth_block();
            self.transactions
                .entry(eth_block.0 as u64)
                .or_default()
                .push(tx_into_log(transaction.clone()));
        }
    }

    fn add_diamond_upgrades(&mut self, upgrades: &[(ProtocolUpgrade, u64)]) {
        for (upgrade, eth_block) in upgrades {
            self.diamond_upgrades
                .entry(*eth_block)
                .or_default()
                .push(upgrade_into_diamond_proxy_log(upgrade.clone(), *eth_block));
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
}

#[derive(Clone)]
struct FakeEthClient {
    inner: Arc<RwLock<FakeEthClientData>>,
}

impl FakeEthClient {
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(FakeEthClientData::new())),
        }
    }

    async fn add_transactions(&mut self, transactions: &[L1Tx]) {
        self.inner.write().await.add_transactions(transactions);
    }

    async fn add_diamond_upgrades(&mut self, upgrades: &[(ProtocolUpgrade, u64)]) {
        self.inner.write().await.add_diamond_upgrades(upgrades);
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
impl EthClient for FakeEthClient {
    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        _retries_left: usize,
    ) -> Result<Vec<Log>, Error> {
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
        Ok(logs)
    }

    fn set_topics(&mut self, _topics: Vec<Hash>) {}

    async fn scheduler_vk_hash(&self, _verifier_address: Address) -> Result<H256, Error> {
        Ok(H256::zero())
    }

    async fn finalized_block_number(&self) -> Result<u64, Error> {
        Ok(self.inner.read().await.last_finalized_block_number)
    }
}

fn build_l1_tx(serial_id: u64, eth_block: u64) -> L1Tx {
    L1Tx {
        execute: Execute {
            contract_address: Address::repeat_byte(0x11),
            calldata: vec![1, 2, 3],
            factory_deps: None,
            value: U256::zero(),
        },
        common_data: L1TxCommonData {
            serial_id: PriorityOpId(serial_id),
            sender: [1u8; 20].into(),
            deadline_block: 0,
            eth_hash: [2; 32].into(),
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
            canonical_tx_hash: H256::from_low_u64_le(serial_id),
        },
        received_timestamp_ms: 0,
    }
}

fn build_upgrade_tx(id: ProtocolVersionId, eth_block: u64) -> ProtocolUpgradeTx {
    ProtocolUpgradeTx {
        execute: Execute {
            contract_address: Address::repeat_byte(0x11),
            calldata: vec![1, 2, 3],
            factory_deps: None,
            value: U256::zero(),
        },
        common_data: ProtocolUpgradeTxCommonData {
            upgrade_id: id,
            sender: [1u8; 20].into(),
            eth_hash: [2; 32].into(),
            eth_block,
            gas_limit: Default::default(),
            max_fee_per_gas: Default::default(),
            gas_per_pubdata_limit: 1u32.into(),
            refund_recipient: Address::zero(),
            to_mint: Default::default(),
            canonical_tx_hash: H256::from_low_u64_be(id as u64),
        },
        received_timestamp_ms: 0,
    }
}

#[tokio::test]
async fn test_normal_operation_l1_txs() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        None,
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
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

#[tokio::test]
async fn test_normal_operation_upgrades() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        None,
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
    client
        .add_diamond_upgrades(&[
            (
                ProtocolUpgrade {
                    id: ProtocolVersionId::latest(),
                    tx: None,
                    ..Default::default()
                },
                10,
            ),
            (
                ProtocolUpgrade {
                    id: ProtocolVersionId::next(),
                    tx: Some(build_upgrade_tx(ProtocolVersionId::next(), 18)),
                    ..Default::default()
                },
                18,
            ),
        ])
        .await;
    client.set_last_finalized_block_number(15).await;
    // second upgrade will not be processed, as it has less than 5 confirmations
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_ids = storage.protocol_versions_dal().all_version_ids().await;
    // there should be genesis version and just added version
    assert_eq!(db_ids.len(), 2);
    assert_eq!(db_ids[1], ProtocolVersionId::latest());

    client.set_last_finalized_block_number(20).await;
    // now the second upgrade will be processed
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_ids = storage.protocol_versions_dal().all_version_ids().await;
    assert_eq!(db_ids.len(), 3);
    assert_eq!(db_ids[2], ProtocolVersionId::next());

    // check that tx was saved with the last upgrade
    let tx = storage
        .protocol_versions_dal()
        .get_protocol_upgrade_tx(ProtocolVersionId::next())
        .await
        .unwrap();
    assert_eq!(tx.common_data.upgrade_id, ProtocolVersionId::next());
}

#[tokio::test]
async fn test_gap_in_upgrades() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        None,
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
    client
        .add_diamond_upgrades(&[(
            ProtocolUpgrade {
                id: ProtocolVersionId::next(),
                tx: None,
                ..Default::default()
            },
            10,
        )])
        .await;
    client.set_last_finalized_block_number(15).await;
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_ids = storage.protocol_versions_dal().all_version_ids().await;
    // there should be genesis version and just added version
    assert_eq!(db_ids.len(), 2);

    let previous_version = (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap();
    let next_version = ProtocolVersionId::next();
    assert_eq!(db_ids[0], previous_version);
    assert_eq!(db_ids[1], next_version);
}

#[tokio::test]
async fn test_normal_operation_governance_upgrades() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        Some(governance_contract()),
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
    client
        .add_governance_upgrades(&[
            (
                ProtocolUpgrade {
                    id: ProtocolVersionId::latest(),
                    tx: None,
                    ..Default::default()
                },
                10,
            ),
            (
                ProtocolUpgrade {
                    id: ProtocolVersionId::next(),
                    tx: Some(build_upgrade_tx(ProtocolVersionId::next(), 18)),
                    ..Default::default()
                },
                18,
            ),
        ])
        .await;
    client.set_last_finalized_block_number(15).await;
    // second upgrade will not be processed, as it has less than 5 confirmations
    watcher.loop_iteration(&mut storage).await.unwrap();

    let db_ids = storage.protocol_versions_dal().all_version_ids().await;
    // there should be genesis version and just added version
    assert_eq!(db_ids.len(), 2);
    assert_eq!(db_ids[1], ProtocolVersionId::latest());

    client.set_last_finalized_block_number(20).await;
    // now the second upgrade will be processed
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_ids = storage.protocol_versions_dal().all_version_ids().await;
    assert_eq!(db_ids.len(), 3);
    assert_eq!(db_ids[2], ProtocolVersionId::next());

    // check that tx was saved with the last upgrade
    let tx = storage
        .protocol_versions_dal()
        .get_protocol_upgrade_tx(ProtocolVersionId::next())
        .await
        .unwrap();
    assert_eq!(tx.common_data.upgrade_id, ProtocolVersionId::next());
}

#[tokio::test]
#[should_panic]
async fn test_gap_in_single_batch() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        None,
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
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

#[tokio::test]
#[should_panic]
async fn test_gap_between_batches() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        None,
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
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

#[tokio::test]
async fn test_overlapping_batches() {
    let connection_pool = ConnectionPool::test_pool().await;
    setup_db(&connection_pool).await;

    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        Address::default(),
        None,
        client.clone(),
        &connection_pool,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_storage().await.unwrap();
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

async fn get_all_db_txs(storage: &mut StorageProcessor<'_>) -> Vec<Transaction> {
    storage.transactions_dal().reset_mempool().await;
    storage
        .transactions_dal()
        .sync_mempool(vec![], vec![], 0, 0, 1000)
        .await
        .0
}

fn tx_into_log(tx: L1Tx) -> Log {
    let eth_block = tx.eth_block().0.into();

    let tx_data_token = Token::Tuple(vec![
        Token::Uint(0xff.into()),
        Token::Address(tx.common_data.sender),
        Token::Address(tx.execute.contract_address),
        Token::Uint(tx.common_data.gas_limit),
        Token::Uint(tx.common_data.gas_per_pubdata_limit),
        Token::Uint(tx.common_data.max_fee_per_gas),
        Token::Uint(U256::zero()),
        Token::Address(Address::zero()),
        Token::Uint(tx.common_data.serial_id.0.into()),
        Token::Uint(tx.execute.value),
        Token::FixedArray(vec![
            Token::Uint(U256::zero()),
            Token::Uint(U256::zero()),
            Token::Uint(U256::zero()),
            Token::Uint(U256::zero()),
        ]),
        Token::Bytes(tx.execute.calldata),
        Token::Bytes(Vec::new()),
        Token::Array(Vec::new()),
        Token::Bytes(Vec::new()),
        Token::Bytes(Vec::new()),
    ]);

    let data = encode(&[
        Token::Uint(tx.common_data.serial_id.0.into()),
        Token::FixedBytes(H256::random().0.to_vec()),
        Token::Uint(u64::MAX.into()),
        tx_data_token,
        Token::Array(Vec::new()),
    ]);

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![zksync_contract()
            .event("NewPriorityRequest")
            .expect("NewPriorityRequest event is missing in abi")
            .signature()],
        data: data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(eth_block),
        transaction_hash: Some(H256::random()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
    }
}

fn upgrade_into_diamond_proxy_log(upgrade: ProtocolUpgrade, eth_block: u64) -> Log {
    let diamond_cut = upgrade_into_diamond_cut(upgrade);
    let data = encode(&[diamond_cut, Token::FixedBytes(vec![0u8; 32])]);
    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![UPGRADE_PROPOSAL_SIGNATURE],
        data: data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(eth_block.into()),
        transaction_hash: Some(H256::random()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
    }
}

fn upgrade_into_governor_log(upgrade: ProtocolUpgrade, eth_block: u64) -> Log {
    let diamond_cut = upgrade_into_diamond_cut(upgrade);
    let execute_upgrade_selector = zksync_contract()
        .function("executeUpgrade")
        .unwrap()
        .short_signature();
    let diamond_upgrade_calldata = execute_upgrade_selector
        .iter()
        .copied()
        .chain(encode(&[diamond_cut]))
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
    let final_data = encode(&[Token::FixedBytes(vec![0u8; 32]), governance_operation]);

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
    }
}

fn upgrade_into_diamond_cut(upgrade: ProtocolUpgrade) -> Token {
    let tx_data_token = if let Some(tx) = upgrade.tx {
        Token::Tuple(vec![
            Token::Uint(0xfe.into()),
            Token::Address(tx.common_data.sender),
            Token::Address(tx.execute.contract_address),
            Token::Uint(tx.common_data.gas_limit),
            Token::Uint(tx.common_data.gas_per_pubdata_limit),
            Token::Uint(tx.common_data.max_fee_per_gas),
            Token::Uint(U256::zero()),
            Token::Address(Address::zero()),
            Token::Uint((tx.common_data.upgrade_id as u16).into()),
            Token::Uint(tx.execute.value),
            Token::FixedArray(vec![
                Token::Uint(U256::zero()),
                Token::Uint(U256::zero()),
                Token::Uint(U256::zero()),
                Token::Uint(U256::zero()),
            ]),
            Token::Bytes(tx.execute.calldata),
            Token::Bytes(Vec::new()),
            Token::Array(Vec::new()),
            Token::Bytes(Vec::new()),
            Token::Bytes(Vec::new()),
        ])
    } else {
        Token::Tuple(vec![
            Token::Uint(0.into()),
            Token::Address(Default::default()),
            Token::Address(Default::default()),
            Token::Uint(Default::default()),
            Token::Uint(Default::default()),
            Token::Uint(Default::default()),
            Token::Uint(Default::default()),
            Token::Address(Default::default()),
            Token::Uint(Default::default()),
            Token::Uint(Default::default()),
            Token::FixedArray(vec![
                Token::Uint(Default::default()),
                Token::Uint(Default::default()),
                Token::Uint(Default::default()),
                Token::Uint(Default::default()),
            ]),
            Token::Bytes(Default::default()),
            Token::Bytes(Default::default()),
            Token::Array(Default::default()),
            Token::Bytes(Default::default()),
            Token::Bytes(Default::default()),
        ])
    };

    let upgrade_token = Token::Tuple(vec![
        tx_data_token,
        Token::Array(Vec::new()),
        Token::FixedBytes(
            upgrade
                .bootloader_code_hash
                .unwrap_or_default()
                .as_bytes()
                .to_vec(),
        ),
        Token::FixedBytes(
            upgrade
                .default_account_code_hash
                .unwrap_or_default()
                .as_bytes()
                .to_vec(),
        ),
        Token::Address(upgrade.verifier_address.unwrap_or_default()),
        Token::Tuple(vec![
            Token::FixedBytes(
                upgrade
                    .verifier_params
                    .unwrap_or_default()
                    .recursion_node_level_vk_hash
                    .as_bytes()
                    .to_vec(),
            ),
            Token::FixedBytes(
                upgrade
                    .verifier_params
                    .unwrap_or_default()
                    .recursion_leaf_level_vk_hash
                    .as_bytes()
                    .to_vec(),
            ),
            Token::FixedBytes(
                upgrade
                    .verifier_params
                    .unwrap_or_default()
                    .recursion_circuits_set_vks_hash
                    .as_bytes()
                    .to_vec(),
            ),
        ]),
        Token::Bytes(Default::default()),
        Token::Bytes(Default::default()),
        Token::Uint(upgrade.timestamp.into()),
        Token::Uint((upgrade.id as u16).into()),
        Token::Address(Default::default()),
    ]);

    Token::Tuple(vec![
        Token::Array(vec![]),
        Token::Address(Default::default()),
        Token::Bytes(
            vec![0u8; 4]
                .into_iter()
                .chain(encode(&[upgrade_token]))
                .collect(),
        ),
    ])
}

async fn setup_db(connection_pool: &ConnectionPool) {
    connection_pool
        .access_storage()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion {
            id: (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap(),
            ..Default::default()
        })
        .await;
}
