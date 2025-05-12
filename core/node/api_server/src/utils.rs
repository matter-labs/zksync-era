//! Utils specific to the API server.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    thread,
    time::{Duration, Instant},
};

use zksync_dal::{
    transactions_web3_dal::ExtendedTransactionReceipt, Connection, Core, CoreDal, DalError,
};
use zksync_state::LruCache;
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    api::TransactionReceipt,
    get_code_key, get_is_account_key, get_nonce_key, h256_to_u256,
    tx::execute::DeploymentParams,
    utils::{decompose_full_nonce, deployed_address_create, deployed_address_evm_create},
    Address, L2BlockNumber, U256,
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
        if timestamp.is_none_or(|ts| now - ts > self.interval) {
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
/// `contract_address` is not set if `from` is a custom account since custom accounts may customize nonce increment policies
/// and/or other deployment aspects.
///
/// Requires all `receipts` to be from the same L2 block.
pub(crate) async fn fill_transaction_receipts(
    storage: &mut Connection<'_, Core>,
    mut receipts: Vec<ExtendedTransactionReceipt>,
) -> Result<Vec<TransactionReceipt>, Web3Error> {
    receipts.sort_unstable_by_key(|receipt| receipt.inner.transaction_index);

    let deployments = receipts.iter().map(|receipt| {
        if receipt.inner.to.is_none() {
            Some(DeploymentTransactionType::Evm)
        } else if receipt.inner.to == Some(CONTRACT_DEPLOYER_ADDRESS) {
            DeploymentParams::decode(&receipt.calldata.0).map(DeploymentTransactionType::EraVm)
        } else {
            // Not a deployment transaction.
            None
        }
    });
    let deployments: Vec<_> = deployments.collect();

    // Get the AA type for deployment transactions to filter out custom AAs below.
    let deployment_receipts = receipts
        .iter()
        .zip(&deployments)
        .filter_map(|(receipt, deployment)| deployment.is_some().then_some(receipt));
    let account_types = if let Some(first_receipt) = deployment_receipts.clone().next() {
        let block_number = L2BlockNumber(first_receipt.inner.block_number.as_u32());
        let from_addresses = deployment_receipts.map(|receipt| receipt.inner.from);
        get_external_account_types(storage, from_addresses, block_number).await?
    } else {
        HashMap::new()
    };

    let mut filled_receipts = Vec::with_capacity(receipts.len());
    let mut receipt_indexes_with_unknown_nonce = HashSet::new();
    for (i, (mut receipt, mut deployment)) in receipts.into_iter().zip(deployments).enumerate() {
        if deployment.is_some()
            && matches!(
                account_types[&receipt.inner.from],
                ExternalAccountType::Custom
            )
        {
            // Custom AAs may interpret transaction data in an arbitrary way (or, more realistically, use a custom
            // nonce increment scheme). Hence, we don't even try to assign `contract_address` for a receipt from a custom AA.
            deployment = None;
        }

        receipt.inner.contract_address = deployment.and_then(|deployment| match deployment {
            DeploymentTransactionType::Evm => Some(deployed_address_evm_create(
                receipt.inner.from,
                receipt.nonce,
            )),
            DeploymentTransactionType::EraVm(
                DeploymentParams::Create | DeploymentParams::CreateAccount,
            ) => {
                // We need a deployment nonce which isn't available locally; we'll compute it in a single batch below.
                receipt_indexes_with_unknown_nonce.insert(i);
                None
            }
            DeploymentTransactionType::EraVm(
                DeploymentParams::Create2(data) | DeploymentParams::Create2Account(data),
            ) => Some(data.derive_address(receipt.inner.from)),
        });
        filled_receipts.push(receipt.inner);
    }

    if let Some(&first_idx) = receipt_indexes_with_unknown_nonce.iter().next() {
        let block_number = L2BlockNumber(filled_receipts[first_idx].block_number.as_u32());
        // We cannot iterate over `receipt_indexes_with_unknown_nonce` since it would violate Rust aliasing rules
        // (Rust isn't smart enough to understand that `receipt_indexes_with_unknown_nonce` are unique).
        let unknown_receipts = filled_receipts
            .iter_mut()
            .enumerate()
            .filter_map(|(i, receipt)| {
                receipt_indexes_with_unknown_nonce
                    .contains(&i)
                    .then_some(receipt)
            })
            .collect();
        fill_receipts_with_unknown_nonce(storage, block_number, unknown_receipts).await?;
    }

    Ok(filled_receipts)
}

#[derive(Debug)]
enum DeploymentTransactionType {
    Evm,
    EraVm(DeploymentParams),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ExternalAccountType {
    Default,
    Custom,
}

async fn get_external_account_types(
    storage: &mut Connection<'_, Core>,
    addresses: impl Iterator<Item = Address>,
    block_number: L2BlockNumber,
) -> Result<HashMap<Address, ExternalAccountType>, Web3Error> {
    let code_keys_to_addresses: HashMap<_, _> = addresses
        .map(|from| (get_code_key(&from).hashed_key(), from))
        .collect();

    // It's fine to query values at the end of the block since the contract type never changes.
    let code_keys: Vec<_> = code_keys_to_addresses.keys().copied().collect();
    let storage_values = storage
        .storage_logs_dal()
        .get_storage_values(&code_keys, block_number)
        .await
        .map_err(DalError::generalize)?;

    Ok(code_keys_to_addresses
        .into_iter()
        .map(|(code_key, address)| {
            let value = storage_values
                .get(&code_key)
                .copied()
                .flatten()
                .unwrap_or_default();
            // The code key slot is non-zero for custom AAs
            let account_type = if value.is_zero() {
                ExternalAccountType::Default
            } else {
                ExternalAccountType::Custom
            };
            (address, account_type)
        })
        .collect())
}

async fn fill_receipts_with_unknown_nonce(
    storage: &mut Connection<'_, Core>,
    block_number: L2BlockNumber,
    receipts: Vec<&mut TransactionReceipt>,
) -> Result<(), Web3Error> {
    // Load nonces at the start of the block.
    let nonce_keys: Vec<_> = receipts
        .iter()
        .map(|receipt| get_nonce_key(&receipt.from).hashed_key())
        .collect();
    let nonces_at_block_start = storage
        .storage_logs_dal()
        .get_storage_values(&nonce_keys, block_number - 1)
        .await
        .map_err(DalError::generalize)?;

    // Load deployment events for the block in order to compute deployment nonces at the start of each transaction.
    // TODO: can filter by the senders as well if necessary.
    let deployment_events = storage
        .events_web3_dal()
        .get_contract_deployment_logs(block_number)
        .await
        .map_err(DalError::generalize)?;

    for (receipt, nonce_key) in receipts.into_iter().zip(nonce_keys) {
        let sender = receipt.from;
        let tx_index = receipt.transaction_index.as_u64();
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
        receipt.contract_address = Some(deployed_address_create(sender, deploy_nonce));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AccountType {
    /// Externally owned account.
    External(ExternalAccountType),
    /// Non-AA contract.
    Contract,
}

impl AccountType {
    fn is_default(self) -> bool {
        matches!(self, Self::External(ExternalAccountType::Default))
    }

    pub(crate) fn is_external(self) -> bool {
        matches!(self, Self::External(_))
    }
}

impl AccountType {
    async fn with_full_nonce(
        storage: &mut Connection<'_, Core>,
        address: Address,
        block_number: L2BlockNumber,
    ) -> Result<(Self, U256), Web3Error> {
        let code_key = get_code_key(&address).hashed_key();
        let is_account_key = get_is_account_key(&address).hashed_key();
        let nonce_key = get_nonce_key(&address).hashed_key();

        let values = storage
            .storage_logs_dal()
            .get_storage_values(&[nonce_key, code_key, is_account_key], block_number)
            .await
            .map_err(DalError::generalize)?;
        let full_nonce = values[&nonce_key].unwrap_or_default();
        let code_hash = values[&code_key].unwrap_or_default();
        let account_info = values[&is_account_key].unwrap_or_default();

        let ty = if code_hash.is_zero() {
            Self::External(ExternalAccountType::Default)
        } else if account_info.is_zero() {
            Self::Contract
        } else {
            Self::External(ExternalAccountType::Custom)
        };
        Ok((ty, h256_to_u256(full_nonce)))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AccountTypesCache {
    cache: LruCache<Address, AccountType>,
}

impl Default for AccountTypesCache {
    fn default() -> Self {
        Self {
            cache: LruCache::uniform("account_types", 1 << 20 /* 1 MiB */),
        }
    }
}

impl AccountTypesCache {
    /// Returns the account type and the stored nonce for the specified address. The nonce is intelligently
    /// selected between the account nonce and deployment nonce, depending on the account type (contract or EOA).
    pub(crate) async fn get_with_nonce(
        &self,
        storage: &mut Connection<'_, Core>,
        address: Address,
        block_number: L2BlockNumber,
    ) -> Result<(AccountType, U256), Web3Error> {
        let (ty, full_nonce) = if let Some(ty) = self.cache.get(&address) {
            let nonce_key = get_nonce_key(&address).hashed_key();
            let full_nonce = storage
                .storage_web3_dal()
                .get_historical_value_unchecked(nonce_key, block_number)
                .await
                .map_err(DalError::generalize)?;
            (ty, h256_to_u256(full_nonce))
        } else {
            let (ty, full_nonce) =
                AccountType::with_full_nonce(storage, address, block_number).await?;
            if !ty.is_default() || !full_nonce.is_zero() {
                // There's activity for the account in question; its type can be cached since it cannot change in the future.
                self.cache.insert(address, ty);
            }
            (ty, full_nonce)
        };

        let (account_nonce, deployment_nonce) = decompose_full_nonce(full_nonce);
        let effective_nonce = match ty {
            AccountType::Contract => deployment_nonce,
            AccountType::External(_) => account_nonce,
        };
        Ok((ty, effective_nonce))
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use zksync_dal::{events_web3_dal::ContractDeploymentLog, ConnectionPool};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_test_contracts::{Account, TestContract, TxType};
    use zksync_types::{
        ethabi, utils::storage_key_for_eth_balance, Address, Execute, ExecuteTransactionCommon,
        StorageLog, Transaction, H256,
    };

    use super::*;
    use crate::testonly::{
        default_fee, persist_block_with_transactions, StateBuilder, TestAccount,
    };

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
    async fn contract_address_not_filled_for_bogus_deployment() {
        let mut alice = Account::random();
        let mut calldata = Execute::encode_deploy_params_create(H256::zero(), H256::zero(), vec![]);
        // Truncate the calldata so that it contains the valid Solidity selector for `create()`, but cannot be decoded.
        calldata.truncate(5);
        let bogus_deployment = alice.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
                calldata,
                value: 0.into(),
                factory_deps: vec![],
            },
            None,
        );
        let deployment_hash = bogus_deployment.hash();

        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        prepare_storage(&mut storage, alice.address()).await;
        persist_block_with_transactions(&pool, vec![bogus_deployment]).await;

        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&[deployment_hash])
            .await
            .unwrap();
        let filled_receipts = fill_transaction_receipts(&mut storage, receipts)
            .await
            .unwrap();

        assert_eq!(filled_receipts.len(), 1);
        assert_eq!(filled_receipts[0].to, Some(CONTRACT_DEPLOYER_ADDRESS));
        assert_eq!(filled_receipts[0].contract_address, None);
        assert_eq!(filled_receipts[0].status, 0.into());
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

    /// Because of AA support, determining deployment nonces is non-trivial. E.g., it would be incorrect
    /// to define a deployment nonce increment as a count of successful deployment txs from an EOA because
    /// the account might have a non-default AA that can be called as a contract (so it can deploy contracts
    /// outside of deployment txs).
    #[tokio::test]
    async fn deployments_with_custom_account() {
        let mut alice = Account::random();
        let (account_deploy_tx, account_addr) =
            alice.create2_account(TestContract::permissive_account().bytecode.to_vec());

        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        StateBuilder::default()
            .with_balance(alice.address(), u64::MAX.into())
            .apply(storage)
            .await;

        let deploy_execute = Execute {
            contract_address: Some(account_addr),
            calldata: TestContract::permissive_account()
                .function("deploy")
                .encode_input(&[ethabi::Token::Uint(5.into())])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![TestContract::permissive_account().dependencies[0]
                .bytecode
                .to_vec()],
        };

        let mut txs = vec![
            // Deploy the account
            account_deploy_tx.into(),
            // Transfer tokens to the created account
            alice
                .create_transfer_with_fee(account_addr, (u64::MAX / 2).into(), default_fee())
                .into(),
            // Trigger deployments from the account by calling it
            alice.get_l2_tx_for_execute(deploy_execute, None),
        ];

        // Trigger a deployment from the AA being the tx initiator
        let mut deploy_tx_from_account = Account::random()
            .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
            .tx;
        let ExecuteTransactionCommon::L2(data) = &mut deploy_tx_from_account.common_data else {
            unreachable!();
        };
        // This invalidates the tx signature, but we don't care because the deployed AA doesn't check it
        data.initiator_address = account_addr;
        txs.push(deploy_tx_from_account);

        let tx_hashes: Vec<_> = txs.iter().map(Transaction::hash).collect();
        let expected_contract_addresses = [Some(account_addr), None, None, None];
        persist_block_with_transactions(&pool, txs).await;

        // Check the account type helpers.
        let mut storage = pool.connection().await.unwrap();
        let account_types = get_external_account_types(
            &mut storage,
            [alice.address, account_addr].into_iter(),
            L2BlockNumber(1),
        )
        .await
        .unwrap();

        assert_matches!(account_types[&alice.address], ExternalAccountType::Default);
        assert_matches!(account_types[&account_addr], ExternalAccountType::Custom);

        let counter_addr = deployed_address_create(account_addr, 0.into());
        let account_types = AccountTypesCache::default();
        // Check 2 times to verify caching logic.
        for _ in 0..2 {
            let (ty, nonce) = account_types
                .get_with_nonce(&mut storage, alice.address, L2BlockNumber(1))
                .await
                .unwrap();
            assert_matches!(ty, AccountType::External(ExternalAccountType::Default));
            assert_eq!(nonce, 3.into()); // 3 first txs in the block
            let (ty, nonce) = account_types
                .get_with_nonce(&mut storage, account_addr, L2BlockNumber(1))
                .await
                .unwrap();
            assert_matches!(ty, AccountType::External(ExternalAccountType::Custom));
            assert_eq!(nonce, 1.into()); // deploying counter
            let (ty, nonce) = account_types
                .get_with_nonce(&mut storage, counter_addr, L2BlockNumber(1))
                .await
                .unwrap();
            assert_matches!(ty, AccountType::Contract);
            assert_eq!(nonce, 0.into());
        }

        // For addresses with no activity, the type should be "the default AA".
        let empty_address = Address::repeat_byte(0xee);
        let (ty, nonce) = account_types
            .get_with_nonce(&mut storage, empty_address, L2BlockNumber(1))
            .await
            .unwrap();
        assert_matches!(ty, AccountType::External(ExternalAccountType::Default));
        assert_eq!(nonce, 0.into());

        assert!(account_types.cache.get(&alice.address).is_some());
        assert!(account_types.cache.get(&account_addr).is_some());
        assert!(account_types.cache.get(&counter_addr).is_some());
        assert!(account_types.cache.get(&empty_address).is_none());

        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&tx_hashes)
            .await
            .unwrap();
        let filled_receipts = fill_transaction_receipts(&mut storage, receipts)
            .await
            .unwrap();
        let contract_addresses: Vec<_> = filled_receipts
            .iter()
            .map(|receipt| {
                assert_eq!(receipt.status, 1.into());
                receipt.contract_address
            })
            .collect();
        assert_eq!(contract_addresses, expected_contract_addresses);
    }

    #[tokio::test]
    async fn getting_nonce_for_evm_contract() {
        let mut alice = Account::random();
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        StateBuilder::default()
            .enable_evm_deployments()
            .with_balance(alice.address(), u64::MAX.into())
            .apply(storage)
            .await;

        let deploy_tx = alice.create_evm_counter_deployment(0.into());
        let counter_address = deployed_address_evm_create(alice.address(), 0.into());
        persist_block_with_transactions(&pool, vec![deploy_tx.into()]).await;

        let account_types = AccountTypesCache::default();
        let mut storage = pool.connection().await.unwrap();
        let (ty, nonce) = account_types
            .get_with_nonce(&mut storage, counter_address, L2BlockNumber(1))
            .await
            .unwrap();
        assert_matches!(ty, AccountType::Contract);
        // EVM contracts get the (deployment) nonce set to 1 on creation.
        assert_eq!(nonce, 1.into());
    }
}
