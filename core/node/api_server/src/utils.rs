//! Utils specific to the API server.

use std::{
    cell::Cell,
    thread,
    time::{Duration, Instant},
};

use zksync_dal::{
    transactions_web3_dal::ExtendedTransactionReceipt, Connection, Core, CoreDal, DalError,
};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    api::TransactionReceipt,
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
        let deployment_events = storage
            .events_web3_dal()
            .get_contract_deployment_logs(requested_block)
            .await
            .map_err(DalError::generalize)?;

        for (idx, nonce_key) in receipt_indexes_with_unknown_nonce
            .into_iter()
            .zip(nonce_keys)
        {
            let sender = filled_receipts[idx].from;
            let tx_index = filled_receipts[idx].transaction_index.as_u64();
            let initial_nonce = nonces_at_block_start
                .get(&nonce_key)
                .copied()
                .flatten()
                .unwrap_or_default();
            let (_, initial_deploy_nonce) = decompose_full_nonce(h256_to_u256(initial_nonce));

            let nonce_increment = deployment_events
                .iter()
                // Can use `take_while` because events are ordered by `transaction_index_in_block`.
                .take_while(|event| event.transaction_index_in_block < tx_index)
                .filter(|event| event.deployer == sender)
                .count();
            let deploy_nonce = initial_deploy_nonce + nonce_increment;
            filled_receipts[idx].contract_address =
                Some(deployed_address_create(sender, deploy_nonce));
        }
    }

    Ok(filled_receipts)
}

#[cfg(test)]
mod tests {
    use zksync_dal::{events_web3_dal::ContractDeploymentLog, ConnectionPool};
    use zksync_multivm::vm_1_3_2::zk_evm_1_3_3::ethereum_types::H256;
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_test_contracts::{Account, TestContract, TxType};
    use zksync_types::{
        ethabi, utils::storage_key_for_eth_balance, Address, Execute, StorageLog, Transaction,
    };

    use super::*;
    use crate::testonly::{persist_block_with_transactions, TestAccount};

    async fn prepare_storage(storage: &mut Connection<'_, Core>, rich_account: Address) {
        insert_genesis_batch(storage, &GenesisParams::mock())
            .await
            .unwrap();
        let balance_key = storage_key_for_eth_balance(&rich_account);
        let balance_log = StorageLog::new_write_log(balance_key, H256::from_low_u64_be(u64::MAX));
        storage
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &[balance_log])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fill_transaction_receipts_basics() {
        let mut alice = Account::random();
        let transfer = alice.create_transfer(1.into());
        let transfer_hash = transfer.hash();
        let deployment = alice
            .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
            .tx;
        let deployment_hash = deployment.hash();

        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        prepare_storage(&mut storage, alice.address()).await;
        persist_block_with_transactions(&pool, vec![transfer.into(), deployment]).await;

        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&[transfer_hash])
            .await
            .unwrap();
        assert_eq!(receipts.len(), 1);
        let filled_receipts = fill_transaction_receipts(&mut storage, receipts)
            .await
            .unwrap();
        assert_eq!(filled_receipts.len(), 1);
        let transfer_receipt = filled_receipts.into_iter().next().unwrap();
        assert_eq!(transfer_receipt.status, 1.into());
        assert_eq!(transfer_receipt.contract_address, None);

        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&[deployment_hash])
            .await
            .unwrap();
        assert_eq!(receipts.len(), 1);
        let filled_receipts = fill_transaction_receipts(&mut storage, receipts)
            .await
            .unwrap();
        assert_eq!(filled_receipts.len(), 1);
        let deploy_receipt = filled_receipts.into_iter().next().unwrap();
        assert_eq!(deploy_receipt.status, 1.into());
        assert_eq!(
            deploy_receipt.contract_address,
            Some(deployed_address_create(alice.address(), 0.into()))
        );
    }

    #[tokio::test]
    async fn various_deployments() {
        let mut alice = Account::random();
        let (create2_execute, create2_params) = Execute::for_create2_deploy(
            H256::zero(),
            TestContract::counter().bytecode.to_vec(),
            &[],
        );

        let txs = vec![
            alice.create_transfer(1.into()).into(),
            alice
                .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
                .tx,
            alice.create_transfer(1.into()).into(),
            // Failed deployment: this should fail with an out-of-gas error due to allocating too many items for reads
            alice
                .get_deploy_tx(
                    TestContract::load_test().bytecode,
                    Some(&[ethabi::Token::Uint(u64::MAX.into())]),
                    TxType::L2,
                )
                .tx,
            alice.get_l2_tx_for_execute(create2_execute.clone(), None),
            // Failed deployment: this tries to deploy to the same address as the previous transaction.
            alice.get_l2_tx_for_execute(create2_execute, None),
            alice
                .get_deploy_tx(
                    TestContract::load_test().bytecode,
                    Some(&[ethabi::Token::Uint(10.into())]),
                    TxType::L2,
                )
                .tx,
        ];
        let tx_hashes: Vec<_> = txs.iter().map(Transaction::hash).collect();
        let expected_statuses_and_contract_addresses = [
            (1_u32, None),
            (1, Some(deployed_address_create(alice.address(), 0.into()))),
            (1, None),
            (0, Some(deployed_address_create(alice.address(), 1.into()))),
            (1, Some(create2_params.derive_address(alice.address()))),
            (0, Some(create2_params.derive_address(alice.address()))),
            // deployment nonce 1 was used by the successful `create2` tx
            (1, Some(deployed_address_create(alice.address(), 2.into()))),
        ];

        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        prepare_storage(&mut storage, alice.address()).await;
        persist_block_with_transactions(&pool, txs).await;

        // Sanity check: for successful deployments, the actual deployed address must correspond to the derived one.
        let deployment_events = storage
            .events_web3_dal()
            .get_contract_deployment_logs(L2BlockNumber(1))
            .await
            .unwrap();
        let expected_events: Vec<_> = expected_statuses_and_contract_addresses
            .iter()
            .enumerate()
            .filter_map(|(i, &(status, address))| {
                if let (1, Some(deployed_address)) = (status, address) {
                    Some(ContractDeploymentLog {
                        transaction_index_in_block: i as u64,
                        deployer: alice.address(),
                        deployed_address,
                    })
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(deployment_events, expected_events);

        for (&tx_hash, &(status, expected_address)) in tx_hashes
            .iter()
            .zip(&expected_statuses_and_contract_addresses)
        {
            println!("Fetching receipt for {tx_hash:?}");
            let receipts = storage
                .transactions_web3_dal()
                .get_transaction_receipts(&[tx_hash])
                .await
                .unwrap();
            let filled_receipts = fill_transaction_receipts(&mut storage, receipts)
                .await
                .unwrap();
            assert_eq!(filled_receipts.len(), 1);
            let receipt = filled_receipts.into_iter().next().unwrap();
            assert_eq!(receipt.status, status.into());
            assert_eq!(receipt.contract_address, expected_address);
        }

        // Test all receipts for a block.
        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&tx_hashes)
            .await
            .unwrap();
        let filled_receipts = fill_transaction_receipts(&mut storage, receipts)
            .await
            .unwrap();
        let statuses_and_contract_addresses: Vec<_> = filled_receipts
            .iter()
            .map(|receipt| (receipt.status.as_u32(), receipt.contract_address))
            .collect();
        assert_eq!(
            statuses_and_contract_addresses,
            expected_statuses_and_contract_addresses
        );
    }
}
