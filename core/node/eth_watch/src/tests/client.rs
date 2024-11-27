use std::{collections::HashMap, convert::TryInto, sync::Arc};

use tokio::sync::RwLock;
use zksync_contracts::{
    chain_admin_contract, hyperchain_contract, state_transition_manager_contract,
};
use zksync_eth_client::{ContractCallError, EnrichedClientResult};
use zksync_types::{
    abi,
    abi::ProposedUpgrade,
    api::{ChainAggProof, Log},
    ethabi,
    ethabi::Token,
    l1::L1Tx,
    u256_to_h256,
    web3::{contract::Tokenizable, BlockNumber},
    Address, L1BatchNumber, L2ChainId, ProtocolUpgrade, SLChainId, Transaction, H256, U256, U64,
};

use crate::client::{EthClient, L2EthClient, RETRY_LIMIT};

#[derive(Debug)]
pub struct FakeEthClientData {
    transactions: HashMap<u64, Vec<Log>>,
    diamond_upgrades: HashMap<u64, Vec<Log>>,
    upgrade_timestamp: HashMap<u64, Vec<Log>>,
    last_finalized_block_number: u64,
    chain_id: SLChainId,
    processed_priority_transactions_count: u64,
    chain_log_proofs: HashMap<L1BatchNumber, ChainAggProof>,
    batch_roots: HashMap<u64, Vec<Log>>,
    chain_roots: HashMap<u64, H256>,
}

impl FakeEthClientData {
    fn new(chain_id: SLChainId) -> Self {
        Self {
            transactions: Default::default(),
            diamond_upgrades: Default::default(),
            upgrade_timestamp: Default::default(),
            last_finalized_block_number: 0,
            chain_id,
            processed_priority_transactions_count: 0,
            chain_log_proofs: Default::default(),
            batch_roots: Default::default(),
            chain_roots: Default::default(),
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

    fn add_upgrade_timestamp(&mut self, upgrades: &[(ProtocolUpgrade, u64)]) {
        for (upgrade, eth_block) in upgrades {
            self.upgrade_timestamp
                .entry(*eth_block)
                .or_default()
                .push(upgrade_timestamp_log(*eth_block));
            self.diamond_upgrades
                .entry(*eth_block)
                .or_default()
                .push(diamond_upgrade_log(upgrade.clone(), *eth_block));
        }
    }

    fn set_last_finalized_block_number(&mut self, number: u64) {
        self.last_finalized_block_number = number;
    }

    fn set_processed_priority_transactions_count(&mut self, number: u64) {
        self.processed_priority_transactions_count = number;
    }

    fn add_batch_roots(&mut self, batch_roots: &[(u64, u64, H256)]) {
        for (sl_block, l2_batch_number, batch_root) in batch_roots {
            self.batch_roots
                .entry(*sl_block)
                .or_default()
                .push(batch_root_to_log(*sl_block, *l2_batch_number, *batch_root));
        }
    }

    fn add_chain_roots(&mut self, chain_roots: &[(u64, H256)]) {
        for (batch, root) in chain_roots {
            self.chain_roots.insert(*batch, *root);
        }
    }

    fn add_chain_log_proofs(&mut self, chain_log_proofs: Vec<(L1BatchNumber, ChainAggProof)>) {
        for (batch, proof) in chain_log_proofs {
            self.chain_log_proofs.insert(batch, proof);
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockEthClient {
    inner: Arc<RwLock<FakeEthClientData>>,
}

impl MockEthClient {
    pub fn new(chain_id: SLChainId) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FakeEthClientData::new(chain_id))),
        }
    }

    pub async fn add_transactions(&mut self, transactions: &[L1Tx]) {
        self.inner.write().await.add_transactions(transactions);
    }

    pub async fn add_upgrade_timestamp(&mut self, upgrades: &[(ProtocolUpgrade, u64)]) {
        self.inner.write().await.add_upgrade_timestamp(upgrades);
    }

    pub async fn set_last_finalized_block_number(&mut self, number: u64) {
        self.inner
            .write()
            .await
            .set_last_finalized_block_number(number);
    }

    pub async fn set_processed_priority_transactions_count(&mut self, number: u64) {
        self.inner
            .write()
            .await
            .set_processed_priority_transactions_count(number)
    }

    pub async fn block_to_number(&self, block: BlockNumber) -> u64 {
        match block {
            BlockNumber::Earliest => 0,
            BlockNumber::Number(number) => number.as_u64(),
            BlockNumber::Pending
            | BlockNumber::Latest
            | BlockNumber::Finalized
            | BlockNumber::Safe => unreachable!(),
        }
    }

    pub async fn add_batch_roots(&mut self, batch_roots: &[(u64, u64, H256)]) {
        self.inner.write().await.add_batch_roots(batch_roots);
    }

    pub async fn add_chain_roots(&mut self, chain_roots: &[(u64, H256)]) {
        self.inner.write().await.add_chain_roots(chain_roots);
    }

    pub async fn add_chain_log_proofs(
        &mut self,
        chain_log_proofs: Vec<(L1BatchNumber, ChainAggProof)>,
    ) {
        self.inner
            .write()
            .await
            .add_chain_log_proofs(chain_log_proofs);
    }
}

#[async_trait::async_trait]
impl EthClient for MockEthClient {
    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topic1: H256,
        topic2: Option<H256>,
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
            if let Some(ops) = self.inner.read().await.upgrade_timestamp.get(&number) {
                logs.extend_from_slice(ops);
            }
            if let Some(ops) = self.inner.read().await.batch_roots.get(&number) {
                logs.extend_from_slice(ops);
            }
        }
        Ok(logs
            .into_iter()
            .filter(|log| {
                log.topics.first() == Some(&topic1)
                    && (topic2.is_none() || log.topics.get(1) == topic2.as_ref())
            })
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

    async fn confirmed_block_number(&self) -> EnrichedClientResult<u64> {
        Ok(self.inner.read().await.last_finalized_block_number)
    }

    async fn diamond_cut_by_version(
        &self,
        packed_version: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        let from_block = *self
            .inner
            .read()
            .await
            .diamond_upgrades
            .keys()
            .min()
            .unwrap_or(&0);
        let to_block = *self
            .inner
            .read()
            .await
            .diamond_upgrades
            .keys()
            .max()
            .unwrap_or(&0);

        let logs = self
            .get_events(
                U64::from(from_block).into(),
                U64::from(to_block).into(),
                state_transition_manager_contract()
                    .event("NewUpgradeCutData")
                    .unwrap()
                    .signature(),
                Some(packed_version),
                RETRY_LIMIT,
            )
            .await?;

        Ok(logs.into_iter().next().map(|log| log.data.0))
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

    async fn get_chain_root(
        &self,
        _block_number: U64,
        _l2_chain_id: L2ChainId,
    ) -> Result<H256, ContractCallError> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl L2EthClient for MockEthClient {
    async fn get_chain_log_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        _chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        Ok(self
            .inner
            .read()
            .await
            .chain_log_proofs
            .get(&l1_batch_number)
            .cloned())
    }

    async fn get_chain_root_l2(
        &self,
        l1_batch_number: L1BatchNumber,
        _l2_chain_id: L2ChainId,
    ) -> Result<Option<H256>, ContractCallError> {
        Ok(self
            .inner
            .read()
            .await
            .chain_roots
            .get(&l1_batch_number.0.into())
            .cloned())
    }
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
        l1_batch_number: None,
        transaction_hash: Some(H256::default()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
        block_timestamp: None,
    }
}

fn init_calldata(protocol_upgrade: ProtocolUpgrade) -> Vec<u8> {
    let upgrade_token = upgrade_into_diamond_cut(protocol_upgrade);

    let encoded_params = ethabi::encode(&[upgrade_token]);

    let execute_upgrade_selector = hyperchain_contract()
        .function("executeUpgrade")
        .unwrap()
        .short_signature();

    // Concatenate the function selector with the encoded parameters
    let mut calldata = Vec::with_capacity(4 + encoded_params.len());
    calldata.extend_from_slice(&execute_upgrade_selector);
    calldata.extend_from_slice(&encoded_params);

    calldata
}

fn diamond_upgrade_log(upgrade: ProtocolUpgrade, eth_block: u64) -> Log {
    // struct DiamondCutData {
    //     FacetCut[] facetCuts;
    //     address initAddress;
    //     bytes initCalldata;
    // }
    let final_data = ethabi::encode(&[Token::Tuple(vec![
        Token::Array(vec![]),
        Token::Address(Address::zero()),
        Token::Bytes(init_calldata(upgrade.clone())),
    ])]);
    tracing::info!("{:?}", Token::Bytes(init_calldata(upgrade)));

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            state_transition_manager_contract()
                .event("NewUpgradeCutData")
                .unwrap()
                .signature(),
            H256::from_low_u64_be(eth_block),
        ],
        data: final_data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(eth_block.into()),
        l1_batch_number: None,
        transaction_hash: Some(H256::random()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
        block_timestamp: None,
    }
}
fn upgrade_timestamp_log(eth_block: u64) -> Log {
    let final_data = ethabi::encode(&[U256::from(12345).into_token()]);

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            chain_admin_contract()
                .event("UpdateUpgradeTimestamp")
                .expect("UpdateUpgradeTimestamp event is missing in ABI")
                .signature(),
            H256::from_low_u64_be(eth_block),
        ],
        data: final_data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(eth_block.into()),
        l1_batch_number: None,
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
    ProposedUpgrade {
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
    .encode()
}

fn batch_root_to_log(sl_block_number: u64, l2_batch_number: u64, batch_root: H256) -> Log {
    let topic1 = ethabi::long_signature(
        "AppendedChainBatchRoot",
        &[
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::FixedBytes(32),
        ],
    );
    let topic2 = u256_to_h256(L2ChainId::default().as_u64().into());
    let topic3 = u256_to_h256(l2_batch_number.into());
    let data = ethabi::encode(&[batch_root.into_token()]);

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![topic1, topic2, topic3],
        data: data.into(),
        block_hash: Some(H256::repeat_byte(0x11)),
        block_number: Some(sl_block_number.into()),
        l1_batch_number: Some(sl_block_number.into()),
        transaction_hash: Some(H256::random()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
        block_timestamp: None,
    }
}
