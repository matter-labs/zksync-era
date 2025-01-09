//! Utils specific to the API server.

use std::{
    cell::Cell,
    thread,
    time::{Duration, Instant},
};

use zksync_dal::{
    transactions_web3_dal::ExtendedTransactionReceipt, Connection, Core, CoreDal, DalError,
};
use zksync_multivm::interface::VmEvent;
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    address_to_h256,
    api::{GetLogsFilter, TransactionReceipt},
    get_nonce_key, h256_to_u256,
    tx::execute::DeploymentParams,
    utils::{decompose_full_nonce, deployed_address_create, deployed_address_evm_create},
    L2BlockNumber,
};
use zksync_web3_decl::error::Web3Error;

/// Opens a readonly transaction over the specified connection.
pub(crate) async fn open_readonly_transaction<'r>(
    conn: &'r mut Connection<'_, Core>,
) -> Result<Connection<'r, Core>, Web3Error> {
    let builder = conn.transaction_builder().map_err(DalError::generalize)?;
    Ok(builder
        .set_readonly()
        .build()
        .await
        .map_err(DalError::generalize)?)
}

/// Allows filtering events (e.g., for logging) so that they are reported no more frequently than with a configurable interval.
///
/// Current implementation uses thread-local vars in order to not rely on mutexes or other cross-thread primitives.
/// I.e., it only really works if the number of threads accessing it is limited (which is the case for the API server;
/// the number of worker threads is congruent to the CPU count).
#[derive(Debug)]
pub(super) struct ReportFilter {
    interval: Duration,
    last_timestamp: &'static thread::LocalKey<Cell<Option<Instant>>>,
}

impl ReportFilter {
    // Should only be used from the `report_filter!` macro.
    pub const fn new(
        interval: Duration,
        last_timestamp: &'static thread::LocalKey<Cell<Option<Instant>>>,
    ) -> Self {
        Self {
            interval,
            last_timestamp,
        }
    }

    /// Should be called sparingly, since it involves moderately heavy operations (getting current time).
    pub fn should_report(&self) -> bool {
        let timestamp = self.last_timestamp.get();
        let now = Instant::now();
        if timestamp.map_or(true, |ts| now - ts > self.interval) {
            self.last_timestamp.set(Some(now));
            true
        } else {
            false
        }
    }
}

/// Creates a new filter with the specified reporting interval *per thread*.
macro_rules! report_filter {
    ($interval:expr) => {{
        thread_local! {
            static LAST_TIMESTAMP: std::cell::Cell<Option<std::time::Instant>> = std::cell::Cell::new(None);
        }
        ReportFilter::new($interval, &LAST_TIMESTAMP)
    }};
}

/// Fills `contract_address` for the provided transaction receipts. This field must be set
/// only for canonical deployment transactions, i.e., txs with `to == None` for EVM contract deployment
/// and calls to `ContractDeployer.{create, create2, createAccount, create2Account}` for EraVM contracts.
/// Also, it must be set regardless of whether the deployment succeeded (thus e.g. we cannot rely on `ContractDeployed` events
/// emitted by `ContractDeployer` for EraVM contracts).
///
/// Requires all `receipts` to be from the same L2 block.
pub(crate) async fn fill_transaction_receipts(
    storage: &mut Connection<'_, Core>,
    mut receipts: Vec<ExtendedTransactionReceipt>,
) -> Result<Vec<TransactionReceipt>, Web3Error> {
    receipts.sort_unstable_by_key(|receipt| receipt.inner.transaction_index);

    let mut filled_receipts = Vec::with_capacity(receipts.len());
    let mut receipt_indexes_with_unknown_nonce = vec![];
    for (i, mut receipt) in receipts.into_iter().enumerate() {
        receipt.inner.contract_address = if receipt.inner.to.is_none() {
            // This is an EVM deployment transaction.
            Some(deployed_address_evm_create(
                receipt.inner.from,
                receipt.nonce,
            ))
        } else if receipt.inner.to == Some(CONTRACT_DEPLOYER_ADDRESS) {
            // Possibly an EraVM deployment transaction.
            if let Some(deployment_params) = DeploymentParams::decode(&receipt.calldata.0)? {
                match deployment_params {
                    DeploymentParams::Create | DeploymentParams::CreateAccount => {
                        // We need a deployment nonce which isn't available locally; we'll compute it in a single batch below.
                        receipt_indexes_with_unknown_nonce.push(i);
                        None
                    }
                    DeploymentParams::Create2(data) | DeploymentParams::Create2Account(data) => {
                        Some(data.derive_address(receipt.inner.from))
                    }
                }
            } else {
                None
            }
        } else {
            // Not a deployment transaction.
            None
        };
        filled_receipts.push(receipt.inner);
    }

    if let Some(&first_idx) = receipt_indexes_with_unknown_nonce.first() {
        let requested_block = L2BlockNumber(filled_receipts[first_idx].block_number.as_u32());
        let nonce_keys: Vec<_> = receipt_indexes_with_unknown_nonce
            .iter()
            .map(|&i| get_nonce_key(&filled_receipts[i].from).hashed_key())
            .collect();

        // Load nonces at the start of the block.
        let nonces_at_block_start = storage
            .storage_logs_dal()
            .get_storage_values(&nonce_keys, requested_block - 1)
            .await
            .map_err(DalError::generalize)?;
        // Load deployment events for the block in order to compute deployment nonces at the start of each transaction.
        // TODO: can filter by the senders as well if necessary.
        let log_filter = GetLogsFilter {
            from_block: requested_block,
            to_block: requested_block,
            addresses: vec![CONTRACT_DEPLOYER_ADDRESS],
            topics: vec![(1, vec![VmEvent::DEPLOY_EVENT_SIGNATURE])],
        };
        let deployment_events = storage
            .events_web3_dal()
            .get_logs(log_filter, usize::MAX)
            .await
            .map_err(DalError::generalize)?;

        for (idx, nonce_key) in receipt_indexes_with_unknown_nonce
            .into_iter()
            .zip(nonce_keys)
        {
            let sender = filled_receipts[idx].from;
            let initial_nonce = nonces_at_block_start
                .get(&nonce_key)
                .copied()
                .flatten()
                .unwrap_or_default();
            let (_, initial_deploy_nonce) = decompose_full_nonce(h256_to_u256(initial_nonce));

            let sender_topic = address_to_h256(&sender);
            let nonce_increment = deployment_events
                .iter()
                // Can use `take_while` because events are ordered by `transaction_index`. `unwrap()` is safe
                // since all events belonging to a block have `transaction_index` set.
                .take_while(|event| event.transaction_index.unwrap() < idx.into())
                .filter(|event| {
                    // First topic in `ContractDeployed` event is the deployer address.
                    event.topics[1] == sender_topic
                })
                .count();
            let deploy_nonce = initial_deploy_nonce + nonce_increment;
            filled_receipts[idx].contract_address =
                Some(deployed_address_create(sender, deploy_nonce));
        }
    }

    Ok(filled_receipts)
}
