use std::collections::HashMap;

use itertools::{Either, Itertools};
use zk_ee::common_structs::{L2ToL1Log, PreimageType};
use zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties as BoojumAccountProperties;
use zk_os_forward_system::run::{
    output::BlockHeader, result_keeper::TxProcessingOutputOwned, BatchOutput,
};
use zksync_types::{
    block::L1BatchTreeData, boojum_os::AccountProperties, fee_model::BatchFeeInput,
    priority_op_onchain_data::PriorityOpOnchainData, AccountTreeId, Address,
    ExecuteTransactionCommon, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey,
    StorageLog, StorageLogKind, Transaction, H256, U256,
};
use zksync_vm_interface::{TransactionExecutionResult, TxExecutionStatus, VmEvent, VmRevertReason};
use zksync_zkos_vm_runner::zkos_conversions::{
    b160_to_address, bytes32_to_h256, zkos_log_to_vm_event,
};

use crate::io::IoCursor;

#[derive(Debug, Clone)]
pub struct UpdatesManager {
    pub l1_batch_number: L1BatchNumber,
    pub l2_block_number: L2BlockNumber,
    pub timestamp: u64,
    pub timestamp_ms: u128,
    pub fee_account_address: Address,
    pub batch_fee_input: BatchFeeInput,
    pub base_fee_per_gas: u64,
    pub protocol_version: ProtocolVersionId,
    pub gas_limit: u64,

    // todo: VM returns events and l2 l1 logs grouped by transaction -
    // reflect it here instead of grouping again later
    pub events: Vec<VmEvent>,
    pub storage_logs: Vec<StorageLog>,
    pub user_l2_to_l1_logs: Vec<L2ToL1Log>,
    pub new_factory_deps: HashMap<H256, Vec<u8>>,
    pub new_account_data: Vec<(H256, AccountProperties)>,
    pub priority_ops_onchain_data: Vec<PriorityOpOnchainData>,
    // only filled in after batch sealed
    // todo: poor abstraction - state transition in this component is not represented on type level
    pub block_header: Option<BlockHeader>,

    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub cumulative_payload_encoding_size: usize,
    pub cumulative_gas_used: u64,
    // only filled in after batch sealed
    // todo: poor abstraction - state transition in this component is not represented on type level
    pub block_pubdata: Option<Vec<u8>>,

    // todo: this shouldn't be set in state keeper - set in tree instead
    pub tree_data: Option<L1BatchTreeData>,
}

impl UpdatesManager {
    pub fn new(
        l1_batch_number: L1BatchNumber,
        l2_block_number: L2BlockNumber,
        timestamp_ms: u128,
        fee_account_address: Address,
        batch_fee_input: BatchFeeInput,
        base_fee_per_gas: u64,
        protocol_version: ProtocolVersionId,
        gas_limit: u64,
    ) -> Self {
        Self {
            l1_batch_number,
            l2_block_number,
            timestamp: u64::try_from(timestamp_ms / 1000).unwrap(),
            timestamp_ms,
            fee_account_address,
            batch_fee_input,
            base_fee_per_gas,
            protocol_version,
            gas_limit,
            events: Vec::new(),
            storage_logs: Vec::new(),
            user_l2_to_l1_logs: Vec::new(),
            new_factory_deps: HashMap::new(),
            new_account_data: Vec::new(),
            block_header: None,
            executed_transactions: Vec::new(),
            cumulative_payload_encoding_size: 0,
            cumulative_gas_used: 0,
            priority_ops_onchain_data: Vec::new(),
            block_pubdata: None,
            tree_data: None,
        }
    }

    pub(crate) fn io_cursor(&self) -> IoCursor {
        IoCursor {
            next_l2_block: self.l2_block_number + 1,
            l1_batch: self.l1_batch_number,
        }
    }

    // todo: we temporarily pass block hash here - move it to tree
    pub fn final_extend(&mut self, mut batch_output: BatchOutput, tree_data: L1BatchTreeData) {
        let tx_output_iter = batch_output.tx_results.into_iter().filter_map(|r| r.ok());
        // todo: instead of concatenating them, store grouped by tx
        for (idx, tx_output) in tx_output_iter.enumerate() {
            let location = (self.l1_batch_number, idx as u32);
            let events = tx_output
                .logs
                .into_iter()
                .map(|log| zkos_log_to_vm_event(log, location));
            self.events.extend(events);

            let logs = tx_output.l2_to_l1_logs.into_iter().map(|log| log.log);
            self.user_l2_to_l1_logs.extend(logs);
        }

        let (factory_deps, account_data): (Vec<_>, Vec<_>) = batch_output
            .published_preimages
            .into_iter()
            .partition_map(|(hash, preimage, preimage_type)| match preimage_type {
                PreimageType::Bytecode => Either::Left((bytes32_to_h256(hash), preimage)),
                PreimageType::AccountData => Either::Right((
                    bytes32_to_h256(hash),
                    convert_boojum_account_properties(BoojumAccountProperties::decode(
                        &preimage
                            .try_into()
                            .expect("Preimage should be exactly 124 bytes"),
                    )),
                )),
            });
        self.new_factory_deps = factory_deps.into_iter().collect();
        self.new_account_data = account_data;

        let storage_logs = batch_output
            .storage_writes
            .into_iter()
            .map(|write| StorageLog {
                kind: StorageLogKind::InitialWrite,
                key: StorageKey::new(
                    AccountTreeId::new(b160_to_address(write.account)),
                    bytes32_to_h256(write.account_key),
                ),
                value: bytes32_to_h256(write.value),
            })
            .collect();
        self.storage_logs = storage_logs;

        self.tree_data = Some(tree_data);
        self.block_header = Some(batch_output.header);
        self.block_pubdata = Some(batch_output.pubdata);
    }

    pub fn extend_from_executed_transaction(
        &mut self,
        transaction: Transaction,
        tx_output: TxProcessingOutputOwned,
    ) {
        self.cumulative_payload_encoding_size += zksync_protobuf::repr::encode::<
            zksync_dal::consensus::proto::Transaction,
        >(&transaction)
        .len();

        self.cumulative_gas_used += tx_output.gas_used;

        let (execution_status, revert_reason) = if tx_output.status {
            (TxExecutionStatus::Success, None)
        } else {
            let revert_reason = VmRevertReason::from(tx_output.output.as_slice()).to_string();
            (TxExecutionStatus::Failure, Some(revert_reason))
        };
        let gas_limit = transaction.gas_limit().as_u64();
        let refunded_gas = gas_limit - tx_output.gas_used;

        if let ExecuteTransactionCommon::L1(data) = &transaction.common_data {
            let onchain_metadata = data.onchain_metadata().onchain_data;
            self.priority_ops_onchain_data.push(onchain_metadata);
        }

        let executed_transaction = TransactionExecutionResult {
            hash: transaction.hash(),
            transaction,
            execution_info: Default::default(),
            execution_status,
            refunded_gas,
            call_traces: Vec::new(),
            revert_reason,
        };
        self.executed_transactions.push(executed_transaction);
    }
}

#[derive(Debug)]
pub struct FinishedBlock {
    pub inner: UpdatesManager,
    pub initial_writes: Vec<H256>,
}

#[derive(Debug)]
pub struct BlockSealCommand {
    pub inner: UpdatesManager,
    pub initial_writes: Vec<H256>,
    /// Whether transactions should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions in DB
    /// before they are included into L2 blocks.
    pub pre_insert_txs: bool,
}

fn convert_boojum_account_properties(p: BoojumAccountProperties) -> AccountProperties {
    AccountProperties {
        versioning_data: p.versioning_data.into_u64(),
        nonce: p.nonce,
        observable_bytecode_hash: bytes32_to_h256(p.observable_bytecode_hash),
        bytecode_hash: bytes32_to_h256(p.bytecode_hash),
        nominal_token_balance: U256::from_big_endian(&p.balance.to_be_bytes::<32>()),
        bytecode_len: p.bytecode_len,
        artifacts_len: p.artifacts_len,
        observable_bytecode_len: p.observable_bytecode_len,
    }
}
