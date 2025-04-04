use std::collections::HashMap;

use itertools::{Either, Itertools};
use zk_ee::common_structs::PreimageType;
use zk_os_basic_system::system_implementation::io::AccountProperties as BoojumAccountProperties;
use zk_os_forward_system::run::{BatchOutput, ExecutionResult, TxOutput};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_state_keeper::io::IoCursor;
use zksync_types::{
    boojum_os::AccountProperties,
    fee_model::BatchFeeInput,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey,
    StorageLog, StorageLogKind, StorageLogWithPreviousValue, Transaction, H256, U256,
};
use zksync_vm_interface::{
    TransactionExecutionResult, TxExecutionStatus, VmEvent, VmExecutionMetrics, VmRevertReason,
};
use zksync_zkos_vm_runner::zkos_conversions::{
    b160_to_address, bytes32_to_h256, zkos_log_to_vm_event,
};

#[derive(Debug, Clone)]
pub struct UpdatesManager {
    pub l1_batch_number: L1BatchNumber,
    pub l2_block_number: L2BlockNumber,
    pub timestamp: u64,
    pub fee_account_address: Address,
    pub batch_fee_input: BatchFeeInput,
    pub base_fee_per_gas: u64,
    pub protocol_version: ProtocolVersionId,
    pub gas_limit: u64,

    pub executed_transactions: Vec<TransactionExecutionResult>,
    pub events: Vec<VmEvent>,
    pub storage_logs: Vec<StorageLog>,
    pub user_l2_to_l1_logs: Vec<UserL2ToL1Log>, // TODO: not filled currently
    pub new_factory_deps: HashMap<H256, Vec<u8>>,
    pub new_account_data: Vec<(H256, AccountProperties)>,
    pub payload_encoding_size: usize,
}

impl UpdatesManager {
    pub fn new(
        l1_batch_number: L1BatchNumber,
        l2_block_number: L2BlockNumber,
        timestamp: u64,
        fee_account_address: Address,
        batch_fee_input: BatchFeeInput,
        base_fee_per_gas: u64,
        protocol_version: ProtocolVersionId,
        gas_limit: u64,
    ) -> Self {
        Self {
            l1_batch_number,
            l2_block_number,
            timestamp,
            fee_account_address,
            batch_fee_input,
            base_fee_per_gas,
            protocol_version,
            gas_limit,
            executed_transactions: Vec::new(),
            events: Vec::new(),
            storage_logs: Vec::new(),
            user_l2_to_l1_logs: Vec::new(),
            new_factory_deps: HashMap::new(),
            new_account_data: Vec::new(),
            payload_encoding_size: 0,
        }
    }

    pub(crate) fn io_cursor(&self) -> IoCursor {
        IoCursor {
            next_l2_block: self.l2_block_number + 1,
            prev_l2_block_hash: H256::zero(),
            prev_l2_block_timestamp: self.timestamp,
            l1_batch: self.l1_batch_number,
        }
    }

    pub fn extend(&mut self, executed_transactions: Vec<Transaction>, batch_output: BatchOutput) {
        let mut next_index_in_batch_output = 0;
        for tx in executed_transactions {
            let tx_output = loop {
                let tx_output = if let Some(tx_result) = batch_output
                    .tx_results
                    .get(next_index_in_batch_output)
                    .cloned()
                {
                    tx_result.ok()
                } else {
                    panic!("No tx result for #{next_index_in_batch_output}");
                };

                next_index_in_batch_output += 1;
                if let Some(tx_output) = tx_output {
                    break tx_output;
                }
            };

            self.extend_from_executed_transaction(tx, tx_output);
        }
        self.extend_from_batch_output(batch_output);
    }

    fn extend_from_executed_transaction(
        &mut self,
        transaction: Transaction,
        zkos_output: TxOutput,
    ) {
        self.payload_encoding_size += zksync_protobuf::repr::encode::<
            zksync_dal::consensus::proto::Transaction,
        >(&transaction)
        .len();

        let location = (
            self.l1_batch_number,
            self.executed_transactions.len() as u32,
        );
        let events = zkos_output
            .logs
            .into_iter()
            .map(|log| zkos_log_to_vm_event(log, location));
        self.events.extend(events);

        let (execution_status, revert_reason) = match &zkos_output.execution_result {
            ExecutionResult::Success(_) => (TxExecutionStatus::Success, None),
            ExecutionResult::Revert(data) => {
                let revert_reason = VmRevertReason::from(data.as_slice()).to_string();
                (TxExecutionStatus::Failure, Some(revert_reason))
            }
        };
        let gas_limit = transaction.gas_limit().as_u64();
        let refunded_gas = gas_limit - zkos_output.gas_used;

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

    fn extend_from_batch_output(&mut self, batch_output: BatchOutput) {
        let (factory_deps, account_data): (Vec<_>, Vec<_>) = batch_output
            .published_preimages
            .into_iter()
            .partition_map(|(hash, preimage, preimage_type)| match preimage_type {
                PreimageType::Bytecode => Either::Left((bytes32_to_h256(hash), preimage)),
                PreimageType::AccountData => Either::Right((
                    bytes32_to_h256(hash),
                    convert_boojum_account_properties(
                        BoojumAccountProperties::decode(
                            preimage
                                .try_into()
                                .expect("Preimage should be exactly 124 bytes"),
                        )
                        .expect("Failed to decode account properties"),
                    ),
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
        nominal_token_balance: U256::from_big_endian(&p.nominal_token_balance.to_be_bytes::<32>()),
        bytecode_len: p.bytecode_len,
        artifacts_len: p.artifacts_len,
        observable_bytecode_len: p.observable_bytecode_len,
    }
}
