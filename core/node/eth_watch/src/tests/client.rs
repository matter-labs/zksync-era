use std::{collections::HashMap, convert::TryInto, sync::Arc};

use tokio::sync::RwLock;
use zksync_contracts::{
    hyperchain_contract, server_notifier_contract, state_transition_manager_contract,
};
use zksync_eth_client::{ContractCallError, EnrichedClientResult};
use zksync_types::{
    abi::{self, ProposedUpgrade, ZkChainSpecificUpgradeData},
    address_to_h256,
    api::{ChainAggProof, Log},
    bytecode::BytecodeHash,
    ethabi::{self, Token},
    h256_to_address, h256_to_u256,
    l1::L1Tx,
    protocol_upgrade::ProtocolUpgradeTx,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
    u256_to_h256,
    utils::encode_ntv_asset_id,
    web3::{contract::Tokenizable, BlockNumber},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolUpgrade, SLChainId, Transaction,
    H256, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256, U64,
};

use crate::client::{EthClient, ScheduledProtocolVersion, ZkSyncExtentionEthClient};

/// Generation of the CTM the mock client emulates. The two generations announce upgrades with
/// different events, and the server is expected to support both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtmGeneration {
    /// `NewUpgradeCutData` is keyed by the new protocol version and there is no
    /// `NewProtocolVersionVerifier` event; the verifier comes from the upgrade calldata.
    #[default]
    Legacy,
    /// `NewUpgradeCutData` is keyed by the old protocol version (plus a backwards-compatible copy
    /// keyed by the new one), and the verifier is announced via `NewProtocolVersionVerifier`.
    Modern,
}

#[derive(Debug)]
pub struct FakeEthClientData {
    ctm_generation: CtmGeneration,
    transactions: HashMap<u64, Vec<Log>>,
    diamond_upgrades: HashMap<u64, Vec<Log>>,
    // `NewProtocolVersion` events mapping an old protocol version (topic1) to a new one (topic2).
    new_protocol_versions: HashMap<u64, Vec<Log>>,
    upgrade_timestamp: HashMap<u64, Vec<Log>>,
    last_finalized_block_number: u64,
    chain_id: SLChainId,
    processed_priority_transactions_count: u64,
    chain_log_proofs: HashMap<L1BatchNumber, ChainAggProof>,
    chain_log_proofs_until_msg_root: HashMap<L2BlockNumber, ChainAggProof>,
    batch_roots: HashMap<u64, Vec<Log>>,
    chain_roots: HashMap<u64, H256>,
    bytecode_preimages: HashMap<H256, Vec<u8>>,
    // `NewProtocolVersionVerifier` events keyed by the packed *new* protocol version.
    protocol_version_verifiers: HashMap<u64, Vec<Log>>,
}

impl FakeEthClientData {
    fn new(chain_id: SLChainId) -> Self {
        Self {
            ctm_generation: CtmGeneration::default(),
            transactions: Default::default(),
            diamond_upgrades: Default::default(),
            new_protocol_versions: Default::default(),
            upgrade_timestamp: Default::default(),
            last_finalized_block_number: 0,
            chain_id,
            processed_priority_transactions_count: 0,
            chain_log_proofs: Default::default(),
            chain_log_proofs_until_msg_root: Default::default(),
            batch_roots: Default::default(),
            chain_roots: Default::default(),
            bytecode_preimages: Default::default(),
            protocol_version_verifiers: Default::default(),
        }
    }

    /// Emits the `NewUpgradeCutData` event(s) that the emulated CTM generation would emit when
    /// scheduling `upgrade` as an upgrade from `old_protocol_version`.
    fn push_diamond_cut_logs(
        &mut self,
        old_protocol_version: ProtocolSemanticVersion,
        upgrade: &ProtocolUpgrade,
        eth_block: u64,
    ) {
        let logs = self.diamond_upgrades.entry(eth_block).or_default();
        if self.ctm_generation == CtmGeneration::Modern {
            logs.push(diamond_upgrade_log(
                old_protocol_version,
                upgrade.clone(),
                eth_block,
            ));
        }
        // Legacy CTMs only emit this one; modern ones emit it for backwards compatibility.
        logs.push(diamond_upgrade_log(
            upgrade.version,
            upgrade.clone(),
            eth_block,
        ));
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
        // Keep in sync with `setup_db()`: tests start from the previous protocol version.
        let mut old_protocol_version = ProtocolSemanticVersion {
            minor: (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap(),
            patch: 0.into(),
        };
        for (upgrade, eth_block) in upgrades {
            self.upgrade_timestamp
                .entry(*eth_block)
                .or_default()
                .push(upgrade_timestamp_log(
                    u256_to_h256(old_protocol_version.pack()),
                    *eth_block,
                ));
            self.push_diamond_cut_logs(old_protocol_version, upgrade, *eth_block);
            self.new_protocol_versions
                .entry(*eth_block)
                .or_default()
                .push(new_protocol_version_log(
                    old_protocol_version,
                    upgrade.version,
                    *eth_block,
                ));
            self.add_bytecode_preimages(&upgrade.tx);
            old_protocol_version = upgrade.version;
        }
    }

    fn add_upgrade_timestamp_for_chain(
        &mut self,
        chain_id: L2ChainId,
        upgrades: &[(ProtocolUpgrade, u64)],
    ) {
        let mut old_protocol_version = ProtocolSemanticVersion {
            minor: (ProtocolVersionId::latest() as u16 - 1).try_into().unwrap(),
            patch: 0.into(),
        };
        for (upgrade, eth_block) in upgrades {
            self.upgrade_timestamp.entry(*eth_block).or_default().push(
                upgrade_timestamp_log_for_chain(
                    chain_id,
                    u256_to_h256(old_protocol_version.pack()),
                    *eth_block,
                ),
            );
            old_protocol_version = upgrade.version;
        }
    }

    fn add_diamond_cut(
        &mut self,
        old_protocol_version: ProtocolSemanticVersion,
        upgrade: ProtocolUpgrade,
        eth_block: u64,
    ) {
        self.add_bytecode_preimages(&upgrade.tx);
        let new_protocol_version = upgrade.version;
        self.push_diamond_cut_logs(old_protocol_version, &upgrade, eth_block);
        self.new_protocol_versions
            .entry(eth_block)
            .or_default()
            .push(new_protocol_version_log(
                old_protocol_version,
                new_protocol_version,
                eth_block,
            ));
    }

    /// Emulates `setUpgradeDiamondCut` on a modern CTM: the cut for an already-scheduled upgrade is
    /// rewritten in place, so only the `NewUpgradeCutData` keyed by the *old* protocol version is
    /// re-emitted. There is no new `NewProtocolVersion`, and no backwards-compatible copy keyed by
    /// the new version — this is the one shape that is discoverable solely under the old-version key.
    fn rewrite_upgrade_cut(
        &mut self,
        old_protocol_version: ProtocolSemanticVersion,
        upgrade: ProtocolUpgrade,
        eth_block: u64,
    ) {
        self.add_bytecode_preimages(&upgrade.tx);
        self.diamond_upgrades
            .entry(eth_block)
            .or_default()
            .push(diamond_upgrade_log(
                old_protocol_version,
                upgrade,
                eth_block,
            ));
    }

    fn add_protocol_version_verifier(
        &mut self,
        new_protocol_version: ProtocolSemanticVersion,
        verifier: Address,
        eth_block: u64,
    ) {
        self.protocol_version_verifiers
            .entry(eth_block)
            .or_default()
            .push(protocol_version_verifier_log(
                new_protocol_version,
                verifier,
                eth_block,
            ));
    }

    fn set_ctm_generation(&mut self, generation: CtmGeneration) {
        self.ctm_generation = generation;
    }

    /// Iterates over logs in `[from_block, confirmed block]`, the range the real client queries the
    /// CTM over. The mock reports the same number as both the confirmed and the finalized block.
    fn confirmed_logs<'a>(
        &self,
        logs: &'a HashMap<u64, Vec<Log>>,
        from_block: u64,
    ) -> impl Iterator<Item = &'a Log> {
        let range = from_block..=self.last_finalized_block_number;
        logs.iter()
            .filter(move |(block_number, _)| range.contains(block_number))
            .flat_map(|(_, logs)| logs)
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

    fn add_chain_log_proofs_until_msg_root(
        &mut self,
        chain_log_proofs_until_msg_root: Vec<(L2BlockNumber, ChainAggProof)>,
    ) {
        for (block, proof) in chain_log_proofs_until_msg_root {
            self.chain_log_proofs_until_msg_root.insert(block, proof);
        }
    }

    fn get_bytecode_preimage(&self, hash: H256) -> Option<Vec<u8>> {
        self.bytecode_preimages.get(&hash).cloned()
    }

    fn add_bytecode_preimages(&mut self, upgrade_tx: &Option<ProtocolUpgradeTx>) {
        let Some(tx) = upgrade_tx.as_ref() else {
            // Nothing to add
            return;
        };

        for dep in tx.execute.factory_deps.iter() {
            self.bytecode_preimages
                .insert(BytecodeHash::for_bytecode(dep).value(), dep.clone());
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

    pub async fn add_upgrade_timestamp_for_chain(
        &mut self,
        chain_id: L2ChainId,
        upgrades: &[(ProtocolUpgrade, u64)],
    ) {
        self.inner
            .write()
            .await
            .add_upgrade_timestamp_for_chain(chain_id, upgrades);
    }

    pub async fn add_diamond_cut(
        &mut self,
        old_protocol_version: ProtocolSemanticVersion,
        upgrade: ProtocolUpgrade,
        eth_block: u64,
    ) {
        self.inner
            .write()
            .await
            .add_diamond_cut(old_protocol_version, upgrade, eth_block);
    }

    pub async fn rewrite_upgrade_cut(
        &mut self,
        old_protocol_version: ProtocolSemanticVersion,
        upgrade: ProtocolUpgrade,
        eth_block: u64,
    ) {
        self.inner
            .write()
            .await
            .rewrite_upgrade_cut(old_protocol_version, upgrade, eth_block);
    }

    pub async fn add_protocol_version_verifier(
        &mut self,
        new_protocol_version: ProtocolSemanticVersion,
        verifier: Address,
        eth_block: u64,
    ) {
        self.inner.write().await.add_protocol_version_verifier(
            new_protocol_version,
            verifier,
            eth_block,
        );
    }

    pub async fn set_ctm_generation(&mut self, generation: CtmGeneration) {
        self.inner.write().await.set_ctm_generation(generation);
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

    pub async fn add_chain_log_proofs_until_msg_root(
        &mut self,
        chain_log_proofs_until_msg_root: Vec<(L2BlockNumber, ChainAggProof)>,
    ) {
        self.inner
            .write()
            .await
            .add_chain_log_proofs_until_msg_root(chain_log_proofs_until_msg_root);
    }
}

#[async_trait::async_trait]
impl EthClient for MockEthClient {
    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topic1: Option<H256>,
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
                log.topics.first() == topic1.as_ref()
                    && (topic2.is_none() || log.topics.get(1) == topic2.as_ref())
            })
            .collect())
    }

    async fn scheduler_vk_hash(
        &self,
        verifier_address: Address,
    ) -> Result<H256, ContractCallError> {
        // Derive the hash from the address so that tests can check which verifier was used.
        Ok(address_to_h256(&verifier_address))
    }

    async fn finalized_block_number(&self) -> EnrichedClientResult<u64> {
        Ok(self.inner.read().await.last_finalized_block_number)
    }

    async fn confirmed_block_number(&self) -> EnrichedClientResult<u64> {
        Ok(self.inner.read().await.last_finalized_block_number)
    }

    async fn scheduled_protocol_version(
        &self,
        old_version: ProtocolSemanticVersion,
    ) -> EnrichedClientResult<Option<ScheduledProtocolVersion>> {
        let packed_old = u256_to_h256(old_version.pack());
        let guard = self.inner.read().await;
        // If several upgrades were scheduled from the same old version, the one in the latest
        // block corresponds to the currently active upgrade path.
        let scheduled = latest_log(
            guard.confirmed_logs(&guard.new_protocol_versions, 0),
            |log| {
                log.topics.first()
                    == Some(
                        &state_transition_manager_contract()
                            .event("NewProtocolVersion")
                            .unwrap()
                            .signature(),
                    )
                    && log.topics.get(1) == Some(&packed_old)
            },
        )
        .map(|log| ScheduledProtocolVersion {
            version: ProtocolSemanticVersion::try_from_packed(h256_to_u256(log.topics[2])).unwrap(),
            block_number: log.block_number.unwrap().as_u64(),
        });
        Ok(scheduled)
    }

    async fn diamond_cut_for_version(
        &self,
        version: ProtocolSemanticVersion,
        from_block: u64,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        let packed_version = u256_to_h256(version.pack());
        let guard = self.inner.read().await;
        // The cut may be rewritten after it was first scheduled, in which case the latest event
        // holds the cut that will actually be applied.
        Ok(latest_log(
            guard.confirmed_logs(&guard.diamond_upgrades, from_block),
            |log| {
                log.topics.first()
                    == Some(
                        &state_transition_manager_contract()
                            .event("NewUpgradeCutData")
                            .unwrap()
                            .signature(),
                    )
                    && log.topics.get(1) == Some(&packed_version)
            },
        )
        .map(|log| log.data.0.clone()))
    }

    async fn verifier_address_for_version(
        &self,
        new_version: ProtocolSemanticVersion,
        from_block: u64,
    ) -> EnrichedClientResult<Option<Address>> {
        let packed_version = u256_to_h256(new_version.pack());
        let guard = self.inner.read().await;
        // If the verifier was set several times, the last event matches the current
        // `protocolVersionVerifier` mapping value.
        Ok(latest_log(
            guard.confirmed_logs(&guard.protocol_version_verifiers, from_block),
            |log| log.topics.get(1) == Some(&packed_version),
        )
        .map(|log| h256_to_address(&log.topics[2])))
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

    async fn get_published_preimages(
        &self,
        hashes: Vec<H256>,
    ) -> EnrichedClientResult<Vec<Option<Vec<u8>>>> {
        let mut result = vec![];

        for hash in hashes {
            result.push(self.inner.read().await.get_bytecode_preimage(hash));
        }

        Ok(result)
    }

    async fn get_chain_gateway_upgrade_info(
        &self,
    ) -> Result<Option<ZkChainSpecificUpgradeData>, ContractCallError> {
        Ok(Some(ZkChainSpecificUpgradeData {
            base_token_asset_id: encode_ntv_asset_id(
                self.chain_id().await?.0.into(),
                SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
            ),
            l2_legacy_shared_bridge: Address::repeat_byte(0x01),
            l2_predeployed_wrapped_base_token: Address::repeat_byte(0x02),
            base_token_l1_address: SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
            base_token_name: String::from("Ether"),
            base_token_symbol: String::from("ETH"),
        }))
    }

    fn bridgehub_addr(&self) -> Option<Address> {
        None
    }

    async fn get_l2_upgrade_tx_data(
        &self,
        _init_address: Address,
        existing_tx_data: Vec<u8>,
    ) -> Result<Vec<u8>, ContractCallError> {
        Ok(existing_tx_data)
    }

    async fn fflonk_scheduler_vk_hash(
        &self,
        _verifier_address: Address,
    ) -> Result<Option<H256>, ContractCallError> {
        Ok(Some(H256::zero()))
    }
}

#[async_trait::async_trait]
impl ZkSyncExtentionEthClient for MockEthClient {
    fn into_base(self: Arc<Self>) -> Arc<dyn EthClient> {
        self
    }

    async fn get_chain_log_proof(
        &self,
        batch_number: L1BatchNumber,
        _chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        Ok(self
            .inner
            .read()
            .await
            .chain_log_proofs
            .get(&batch_number)
            .cloned())
    }

    async fn get_chain_log_proof_until_msg_root(
        &self,
        block_number: L2BlockNumber,
        _chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        Ok(self
            .inner
            .read()
            .await
            .chain_log_proofs_until_msg_root
            .get(&block_number)
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

/// Returns the log matching `predicate` that the CTM emitted last, mirroring how the real client
/// orders logs by `(block_number, log_index)`.
fn latest_log<'a>(
    logs: impl Iterator<Item = &'a Log>,
    predicate: impl Fn(&Log) -> bool,
) -> Option<&'a Log> {
    logs.filter(|log| predicate(log)).max_by_key(|log| {
        (
            log.block_number.unwrap_or_default(),
            log.log_index.unwrap_or_default(),
        )
    })
}

/// Builds a `NewUpgradeCutData` log carrying `upgrade`, keyed by `cut_key_version`, which is the old
/// or the new protocol version depending on the [`CtmGeneration`].
fn diamond_upgrade_log(
    cut_key_version: ProtocolSemanticVersion,
    upgrade: ProtocolUpgrade,
    eth_block: u64,
) -> Log {
    // struct DiamondCutData {
    //     FacetCut[] facetCuts;
    //     address initAddress;
    //     bytes initCalldata;
    // }
    let version = u256_to_h256(cut_key_version.pack());
    let final_data = ethabi::encode(&[Token::Tuple(vec![
        Token::Array(vec![]),
        Token::Address(Address::zero()),
        Token::Bytes(init_calldata(upgrade.clone())),
    ])]);

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            state_transition_manager_contract()
                .event("NewUpgradeCutData")
                .unwrap()
                .signature(),
            version,
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

fn new_protocol_version_log(
    old_protocol_version: ProtocolSemanticVersion,
    new_protocol_version: ProtocolSemanticVersion,
    eth_block: u64,
) -> Log {
    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            state_transition_manager_contract()
                .event("NewProtocolVersion")
                .unwrap()
                .signature(),
            u256_to_h256(old_protocol_version.pack()),
            u256_to_h256(new_protocol_version.pack()),
        ],
        data: Default::default(),
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
fn protocol_version_verifier_log(
    new_protocol_version: ProtocolSemanticVersion,
    verifier: Address,
    eth_block: u64,
) -> Log {
    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            state_transition_manager_contract()
                .event("NewProtocolVersionVerifier")
                .unwrap()
                .signature(),
            u256_to_h256(new_protocol_version.pack()),
            address_to_h256(&verifier),
        ],
        data: Default::default(),
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

fn upgrade_timestamp_log(packed_version: H256, eth_block: u64) -> Log {
    upgrade_timestamp_log_for_chain(L2ChainId::default(), packed_version, eth_block)
}

pub(super) fn upgrade_timestamp_log_for_chain(
    chain_id: L2ChainId,
    packed_version: H256,
    eth_block: u64,
) -> Log {
    let final_data = ethabi::encode(&[U256::from(12345).into_token()]);

    Log {
        address: Address::repeat_byte(0x1),
        topics: vec![
            server_notifier_contract()
                .event("UpgradeTimestampUpdated")
                .expect("UpgradeTimestampUpdated event is missing in ABI")
                .signature(),
            u256_to_h256(chain_id.as_u64().into()),
            packed_version,
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
    let abi::Transaction::L1 { tx, .. } = upgrade
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
    let factory_deps = upgrade.version.minor.is_pre_gateway().then(Vec::new);
    ProposedUpgrade {
        l2_protocol_upgrade_tx: tx,
        factory_deps,
        bootloader_hash: upgrade.bootloader_code_hash.unwrap_or_default().into(),
        default_account_hash: upgrade.default_account_code_hash.unwrap_or_default().into(),
        evm_emulator_hash: upgrade.evm_emulator_code_hash.unwrap_or_default().into(),
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
        l1_batch_number: Some(l2_batch_number.into()),
        transaction_hash: Some(H256::random()),
        transaction_index: Some(0u64.into()),
        log_index: Some(0u64.into()),
        transaction_log_index: Some(0u64.into()),
        log_type: None,
        removed: None,
        block_timestamp: None,
    }
}
