use std::time::Duration;

use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes};
use zksync_types::{Address, L1BatchNumber, MiniblockNumber, Transaction};

use crate::state_keeper::{
    io::{
        common::{l1_batch_params, poll_until},
        L1BatchParams, PendingBatchData, StateKeeperIO,
    },
    seal_criteria::SealerFn,
    updates::UpdatesManager,
};

use super::sync_action::{ActionQueue, SyncAction};

/// The interval between the action queue polling attempts for the new actions.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// In the external node we don't actually decide whether we want to seal l1 batch or l2 block.
/// We must replicate the state as it's present in the main node.
/// This structure declares an "unconditional sealer" which would tell the state keeper to seal
/// blocks/batches at the same point as in the main node.
#[derive(Debug, Clone)]
pub struct ExternalNodeSealer {
    actions: ActionQueue,
}

impl ExternalNodeSealer {
    pub fn new(actions: ActionQueue) -> Self {
        Self { actions }
    }

    fn should_seal_miniblock(&self) -> bool {
        let res = matches!(self.actions.peek_action(), Some(SyncAction::SealMiniblock));
        vlog::info!("Asked if should seal the miniblock. The answer is {res}");
        res
    }

    fn should_seal_batch(&self) -> bool {
        let res = matches!(self.actions.peek_action(), Some(SyncAction::SealBatch));
        vlog::info!("Asked if should seal the batch. The answer is {res}");
        res
    }

    pub fn into_unconditional_batch_seal_criterion(self) -> Box<SealerFn> {
        Box::new(move |_| self.should_seal_batch())
    }

    pub fn into_miniblock_seal_criterion(self) -> Box<SealerFn> {
        Box::new(move |_| self.should_seal_miniblock())
    }
}

/// ExternalIO is the IO abstraction for the state keeper that is used in the external node.
/// It receives a sequence of actions from the fetcher via the action queue and propagates it
/// into the state keeper.
///
/// It is also responsible for the persisting of data, and this slice of logic is pretty close
/// to the one in the mempool IO (which is used in the main node).
#[derive(Debug)]
pub struct ExternalIO {
    fee_account: Address,

    current_l1_batch_number: L1BatchNumber,
    current_miniblock_number: MiniblockNumber,
    actions: ActionQueue,
}

impl ExternalIO {
    pub fn new(fee_account: Address, actions: ActionQueue) -> Self {
        Self {
            fee_account,
            current_l1_batch_number: L1BatchNumber(1),
            current_miniblock_number: MiniblockNumber(1),
            actions,
        }
    }
}

impl StateKeeperIO for ExternalIO {
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        None
    }

    fn wait_for_new_batch_params(&mut self, max_wait: Duration) -> Option<L1BatchParams> {
        vlog::info!("Waiting for the new batch params");
        poll_until(POLL_INTERVAL, max_wait, || {
            match self.actions.pop_action()? {
                SyncAction::OpenBatch {
                    number,
                    timestamp,
                    l1_gas_price,
                    l2_fair_gas_price,
                    base_system_contracts_hashes,
                } => {
                    assert_eq!(
                        number, self.current_l1_batch_number,
                        "Batch number mismatch"
                    );
                    Some(l1_batch_params(
                        number,
                        self.fee_account,
                        timestamp,
                        Default::default(),
                        l1_gas_price,
                        l2_fair_gas_price,
                        load_base_contracts(base_system_contracts_hashes),
                    ))
                }
                other => {
                    panic!("Unexpected action in the action queue: {:?}", other);
                }
            }
        })
    }

    fn wait_for_new_miniblock_params(&mut self, max_wait: Duration) -> Option<u64> {
        // Wait for the next miniblock to appear in the queue.
        poll_until(POLL_INTERVAL, max_wait, || {
            match self.actions.peek_action()? {
                SyncAction::Miniblock { number, timestamp } => {
                    self.actions.pop_action(); // We found the miniblock, remove it from the queue.
                    assert_eq!(
                        number, self.current_miniblock_number,
                        "Miniblock number mismatch"
                    );
                    Some(timestamp)
                }
                SyncAction::SealBatch => {
                    // We've reached the next batch, so this situation would be handled by the batch sealer.
                    // No need to pop the action from the queue.
                    // It also doesn't matter which timestamp we return, since there will be no more miniblocks in this
                    // batch. We return 0 to make it easy to detect if it ever appears somewhere.
                    Some(0)
                }
                other => {
                    panic!(
                        "Unexpected action in the queue while waiting for the next miniblock {:?}",
                        other
                    );
                }
            }
        })
    }

    fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction> {
        vlog::info!(
            "Waiting for the new tx, next action is {:?}",
            self.actions.peek_action()
        );
        poll_until(POLL_INTERVAL, max_wait, || {
            // We keep polling until we get any item from the queue.
            // Once we have the item, it'll be either a transaction, or a seal request.
            // Whatever item it is, we don't have to poll anymore and may exit, thus double option use.
            match self.actions.peek_action()? {
                SyncAction::Tx(_) => {
                    let SyncAction::Tx(tx) = self.actions.pop_action().unwrap() else { unreachable!() };
                    Some(Some(*tx))
                }
                _ => Some(None),
            }
        })?
    }

    fn rollback(&mut self, tx: &Transaction) {
        // We are replaying the already sealed batches so no rollbacks are expected to occur.
        panic!("Rollback requested: {:?}", tx);
    }

    fn reject(&mut self, tx: &Transaction, error: &str) {
        // We are replaying the already executed transactions so no rejections are expected to occur.
        panic!(
            "Reject requested because of the following error: {}.\n Transaction is: {:?}",
            error, tx
        );
    }

    fn seal_miniblock(&mut self, _updates_manager: &UpdatesManager) {
        match self.actions.pop_action() {
            Some(SyncAction::SealMiniblock) => {}
            other => panic!(
                "State keeper requested to seal miniblock, but the next action is {:?}",
                other
            ),
        };
        self.current_miniblock_number += 1;
        vlog::info!("Miniblock {} is sealed", self.current_miniblock_number);
    }

    fn seal_l1_batch(
        &mut self,
        _block_result: vm::VmBlockResult,
        _updates_manager: UpdatesManager,
        _block_context: vm::vm_with_bootloader::DerivedBlockContext,
    ) {
        match self.actions.pop_action() {
            Some(SyncAction::SealBatch) => {}
            other => panic!(
                "State keeper requested to seal the batch, but the next action is {:?}",
                other
            ),
        };
        self.current_l1_batch_number += 1;

        vlog::info!("Batch {} is sealed", self.current_l1_batch_number);
    }
}

/// Currently it always returns the contracts that are present on the disk.
/// Later on, support for different base contracts versions will be added.
fn load_base_contracts(expected_hashes: BaseSystemContractsHashes) -> BaseSystemContracts {
    let base_system_contracts = BaseSystemContracts::load_from_disk();
    let local_hashes = base_system_contracts.hashes();

    assert_eq!(
        local_hashes, expected_hashes,
        "Local base system contract hashes do not match ones required to process the L1 batch"
    );

    base_system_contracts
}
