use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context as _;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_config::configs::chain::MempoolConfig;
use zksync_contracts::{l2_asset_router, l2_legacy_shared_bridge};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_multivm::{
    utils::derive_base_fee_and_gas_per_pubdata, vm_fast::interface::opcodes::Add,
};
use zksync_node_fee_model::BatchFeeModelInputProvider;
use zksync_types::{
    address_to_h256,
    ethabi::{self, Param, ParamType, Token},
    get_address_mapping_key, get_immutable_simulator_key, get_nonce_key, h256_to_address,
    hasher::keccak,
    tx::execute::Create2DeploymentParams,
    utils::encode_ntv_asset_id,
    vm::VmVersion,
    web3::keccak256,
    AccountTreeId, Address, Nonce, StorageKey, Transaction, TransactionTimeRangeConstraint, H256,
    L2_ASSET_ROUTER_ADDRESS, L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_IMMUTABLE_KEY,
    L2_LEGACY_SHARED_BRIDGE_BEACON_PROXY_BYTECODE_KEY, L2_LEGACY_SHARED_BRIDGE_L1_ADDRESSES_KEY,
    L2_NATIVE_TOKEN_VAULT_ADDRESS, L2_NATIVE_TOKEN_VAULT_ASSET_ID_MAPPING_INDEX,
};

use super::{metrics::KEEPER_METRICS, types::MempoolGuard};

/// Creates a mempool filter for L2 transactions based on the current L1 gas price.
/// The filter is used to filter out transactions from the mempool that do not cover expenses
/// to process them.
pub async fn l2_tx_filter(
    batch_fee_input_provider: &dyn BatchFeeModelInputProvider,
    vm_version: VmVersion,
) -> anyhow::Result<L2TxFilter> {
    let fee_input = batch_fee_input_provider.get_batch_fee_input().await?;
    let (base_fee, gas_per_pubdata) = derive_base_fee_and_gas_per_pubdata(fee_input, vm_version);
    Ok(L2TxFilter {
        fee_input,
        fee_per_gas: base_fee,
        gas_per_pubdata: gas_per_pubdata as u32,
    })
}

#[derive(Debug)]
pub struct MempoolFetcher {
    mempool: MempoolGuard,
    pool: ConnectionPool<Core>,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    sync_interval: Duration,
    sync_batch_size: usize,
    stuck_tx_timeout: Option<Duration>,
    #[cfg(test)]
    transaction_hashes_sender: mpsc::UnboundedSender<Vec<H256>>,
}

fn extract_token_from_legacy_deposit(legacy_deposit_data: &[u8]) -> Option<Address> {
    let contract = l2_legacy_shared_bridge();
    let function = contract.function("finalizeDeposit").unwrap();

    let short_sig = function.short_signature();

    if legacy_deposit_data.len() < 4 || short_sig != legacy_deposit_data[..4] {
        return None;
    }

    let decoded = function.decode_input(&legacy_deposit_data[4..]).ok()?;
    decoded[2].clone().into_address()
}

fn extract_token_from_asset_router_deposit(
    l1_chain_id: u64,
    asset_router_deposit_data: &[u8],
) -> Option<Address> {
    let contract = l2_asset_router();

    // There are two types of `finalizeDeposit` functions:
    // - The one with 5 params that maintains the same interface as the one in the legacy contract
    // - The one with 3 params with the new interface.

    // TODO: maybe add a unit test to enforce that there are always two functions.
    let finalize_deposit_functions = contract.functions.get("finalizeDeposit")?;
    let finalize_deposit_3_params = finalize_deposit_functions
        .iter()
        .find(|f| f.inputs.len() == 3)
        .unwrap();
    let finalize_deposit_5_params = finalize_deposit_functions
        .iter()
        .find(|f| f.inputs.len() == 5)
        .unwrap();
    assert_eq!(
        finalize_deposit_functions.len(),
        2,
        "Unexpected ABI of L2AssetRouter"
    );

    if asset_router_deposit_data.len() < 4 {
        return None;
    }

    // Firstly, we test against the 5-input one as it is simpler.
    if finalize_deposit_5_params.short_signature() == asset_router_deposit_data[..4] {
        let decoded = finalize_deposit_5_params
            .decode_input(&asset_router_deposit_data[4..])
            .ok()?;
        return decoded[2].clone().into_address();
    }

    // Now, we test the option when the 3-paramed function is used.
    if finalize_deposit_3_params.short_signature() != asset_router_deposit_data[..4] {
        return None;
    }

    let decoded = finalize_deposit_3_params
        .decode_input(&asset_router_deposit_data[4..])
        .ok()?;
    let used_asset_id = H256::from_slice(&decoded[1].clone().into_fixed_bytes()?);
    let bridge_mint_data = decoded[2].clone().into_bytes()?;

    let decoded_ntv_input_data = ethabi::decode(
        &[
            ParamType::Address,
            ParamType::Address,
            ParamType::Address,
            ParamType::Uint(256),
            ParamType::Bytes,
        ],
        &bridge_mint_data,
    )
    .ok()?;
    let origin_token_address = decoded_ntv_input_data[2].clone().into_address()?;

    // We will also double check that the assetId corresponds to the assetId of a token that is bridged from l1
    let expected_asset_id = encode_ntv_asset_id(l1_chain_id.into(), origin_token_address);

    // The used asset id is wrong, we should not rely on the token address to be processed by the native token vault
    if expected_asset_id != used_asset_id {
        return None;
    }

    Some(origin_token_address)
}

async fn calculate_expected_token_address(
    storage: &mut Connection<'static, Core>,
    l2_legacy_shared_bridge_address: Address,
    l1_token_address: Address,
) -> anyhow::Result<Address> {
    // The source of truth for this logic is `L2SharedBridgeLegacy._calculateCreate2TokenAddress`

    let beacon_proxy_bytecode_hash_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        L2_LEGACY_SHARED_BRIDGE_BEACON_PROXY_BYTECODE_KEY,
    );
    let beacon_proxy_bytecode_hash = storage
        .storage_web3_dal()
        .get_value(&beacon_proxy_bytecode_hash_key)
        .await?;

    let params = Create2DeploymentParams {
        salt: address_to_h256(&l1_token_address),
        bytecode_hash: beacon_proxy_bytecode_hash,
        raw_constructor_input: ethabi::encode(&[
            Token::Address(l1_token_address),
            Token::Bytes(vec![]),
        ]),
    };

    Ok(params.derive_address(l2_legacy_shared_bridge_address))
}

/// Checks whether the token is legacy. It is legacy if both of the
/// following is true:
/// - It is present in legacy shared bridge
/// - It is not present in the L2 native token vault
async fn is_l2_token_legacy(
    storage: &mut Connection<'static, Core>,
    l2_legacy_shared_bridge_address: Address,
    l2_token_address: Address,
    expected_l1_address: Address,
) -> anyhow::Result<bool> {
    // 1. Read l1 token address from L2 shared bridge (must not be 0)

    let stored_l1_address_key = StorageKey::new(
        AccountTreeId::new(l2_legacy_shared_bridge_address),
        get_address_mapping_key(&l2_token_address, L2_LEGACY_SHARED_BRIDGE_L1_ADDRESSES_KEY),
    );
    let stored_l1_address = storage
        .storage_web3_dal()
        .get_value(&stored_l1_address_key)
        .await?;
    let stored_l1_address = h256_to_address(&stored_l1_address);

    // No address is stored, it means that the token has never been bridged before
    // and thus, it is not legacy
    if stored_l1_address == Address::zero() {
        return Ok(false);
    }

    // Just for cross check
    assert_eq!(expected_l1_address, stored_l1_address);

    // 2. Read assetId from NTV (must be 0)
    let stored_asset_id_key = StorageKey::new(
        AccountTreeId::new(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        get_address_mapping_key(
            &l2_token_address,
            L2_NATIVE_TOKEN_VAULT_ASSET_ID_MAPPING_INDEX,
        ),
    );
    let stored_asset_id = storage
        .storage_web3_dal()
        .get_value(&stored_asset_id_key)
        .await?;

    Ok(stored_asset_id == H256::zero())
}

/// Accepts a list of transactions to be included into the mempool and filters
/// the unsafe deposits and returns two vectors:
/// - Transactions to include into the mempool
/// - Transactions to return to the mempool
///
/// Note, that the purpose of this function is not to find unsafe deposits *only*
/// but detect whether they may be present at all. It does check that, e.g. the sender
/// of the transactions is the correct l1 bridge.
async fn is_unsafe_deposit_present(
    txs: &[(Transaction, TransactionTimeRangeConstraint)],
    storage: &mut Connection<'static, Core>,
) -> anyhow::Result<bool> {
    // The rules are the following:
    // - All L2 transactions are allowed
    // - L1 transactions are allowed only if it is a deposit to an already registered token or
    // to a non-legacy token.

    // Firstly, let's check whether the chain has a legacy bridge.
    let legacy_bridge_key = get_immutable_simulator_key(
        &L2_ASSET_ROUTER_ADDRESS,
        L2_ASSET_ROUTER_LEGACY_SHARED_BRIDGE_IMMUTABLE_KEY,
    );

    let legacy_l2_shared_bridge_addr = storage
        .storage_web3_dal()
        .get_value(&legacy_bridge_key)
        .await?;
    let legacy_l2_shared_bridge_addr = h256_to_address(&legacy_l2_shared_bridge_addr);

    // There is either no legacy bridge or the L2AssetRouter has not been depoyed yet.
    // In both cases, there can be no unsafe deposits.
    if legacy_l2_shared_bridge_addr == Address::zero() {
        return Ok(false);
    }

    for (tx, _) in txs {
        if !tx.is_l1() {
            // Not a deposit
            continue;
        }

        let Some(contract_address) = tx.execute.contract_address else {
            continue;
        };

        let l1_token_address = if contract_address == legacy_l2_shared_bridge_addr {
            extract_token_from_legacy_deposit(tx.execute.calldata())
        } else if contract_address == L2_ASSET_ROUTER_ADDRESS {
            // FIXME: chain id not 0
            extract_token_from_asset_router_deposit(0, tx.execute.calldata())
        } else {
            None
        };

        let Some(l1_token_address) = l1_token_address else {
            continue;
        };

        let l2_token_address = calculate_expected_token_address(
            storage,
            legacy_l2_shared_bridge_addr,
            l1_token_address,
        )
        .await?;

        if is_l2_token_legacy(
            storage,
            legacy_l2_shared_bridge_addr,
            l2_token_address,
            l1_token_address,
        )
        .await?
        {
            return Ok(true);
        }
    }

    Ok(false)
}

impl MempoolFetcher {
    pub fn new(
        mempool: MempoolGuard,
        batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
        config: &MempoolConfig,
        pool: ConnectionPool<Core>,
    ) -> Self {
        Self {
            mempool,
            pool,
            batch_fee_input_provider,
            sync_interval: config.sync_interval(),
            sync_batch_size: config.sync_batch_size,
            stuck_tx_timeout: config.remove_stuck_txs.then(|| config.stuck_tx_timeout()),
            #[cfg(test)]
            transaction_hashes_sender: mpsc::unbounded_channel().0,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        if let Some(stuck_tx_timeout) = self.stuck_tx_timeout {
            let removed_txs = storage
                .transactions_dal()
                .remove_stuck_txs(stuck_tx_timeout)
                .await
                .context("failed removing stuck transactions")?;
            tracing::info!("Number of stuck txs was removed: {removed_txs}");
        }
        storage.transactions_dal().reset_mempool().await?;
        drop(storage);

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, mempool is shutting down");
                break;
            }
            let latency = KEEPER_METRICS.mempool_sync.start();
            let mut storage = self.pool.connection_tagged("state_keeper").await?;
            let mempool_info = self.mempool.get_mempool_info();

            KEEPER_METRICS
                .mempool_stashed_accounts
                .set(mempool_info.stashed_accounts.len());
            KEEPER_METRICS
                .mempool_purged_accounts
                .set(mempool_info.purged_accounts.len());

            let protocol_version = storage
                .blocks_dal()
                .pending_protocol_version()
                .await
                .context("failed getting pending protocol version")?;

            let (fee_per_gas, gas_per_pubdata) = if let Some(unsealed_batch) = storage
                .blocks_dal()
                .get_unsealed_l1_batch()
                .await
                .context("failed getting unsealed batch")?
            {
                let (fee_per_gas, gas_per_pubdata) = derive_base_fee_and_gas_per_pubdata(
                    unsealed_batch.fee_input,
                    protocol_version.into(),
                );
                (fee_per_gas, gas_per_pubdata as u32)
            } else {
                let filter = l2_tx_filter(
                    self.batch_fee_input_provider.as_ref(),
                    protocol_version.into(),
                )
                .await
                .context("failed creating L2 transaction filter")?;

                (filter.fee_per_gas, filter.gas_per_pubdata)
            };

            let transactions_with_constraints = storage
                .transactions_dal()
                .sync_mempool(
                    &mempool_info.stashed_accounts,
                    &mempool_info.purged_accounts,
                    gas_per_pubdata,
                    fee_per_gas,
                    true,
                    self.sync_batch_size,
                )
                .await
                .context("failed syncing mempool")?;

            let exclude_l1_txs =
                is_unsafe_deposit_present(&transactions_with_constraints, &mut storage).await?;

            let transactions_with_constraints = if exclude_l1_txs {
                let hashes: Vec<_> = transactions_with_constraints
                    .iter()
                    .map(|x| x.0.hash())
                    .collect();

                storage
                    .transactions_dal()
                    .return_to_mempool(&hashes)
                    .await
                    .context("failed to return txs to mempool")?;

                storage
                    .transactions_dal()
                    .sync_mempool(
                        &mempool_info.stashed_accounts,
                        &mempool_info.purged_accounts,
                        gas_per_pubdata,
                        fee_per_gas,
                        false,
                        self.sync_batch_size,
                    )
                    .await
                    .context("failed syncing mempool")?
            } else {
                transactions_with_constraints
            };

            let transactions: Vec<_> = transactions_with_constraints
                .iter()
                .map(|(t, _c)| t)
                .collect();

            let nonces = get_transaction_nonces(&mut storage, &transactions).await?;
            drop(storage);

            #[cfg(test)]
            {
                let transaction_hashes = transactions.iter().map(|x| x.hash()).collect();
                self.transaction_hashes_sender.send(transaction_hashes).ok();
            }
            let all_transactions_loaded = transactions.len() < self.sync_batch_size;
            self.mempool.insert(transactions_with_constraints, nonces);
            latency.observe();

            if all_transactions_loaded {
                tokio::time::sleep(self.sync_interval).await;
            }
        }
        Ok(())
    }
}

/// Loads nonces for all distinct `transactions` initiators from the storage.
async fn get_transaction_nonces(
    storage: &mut Connection<'_, Core>,
    transactions: &[&Transaction],
) -> anyhow::Result<HashMap<Address, Nonce>> {
    let (nonce_keys, address_by_nonce_key): (Vec<_>, HashMap<_, _>) = transactions
        .iter()
        .map(|tx| {
            let address = tx.initiator_account();
            let nonce_key = get_nonce_key(&address).hashed_key();
            (nonce_key, (nonce_key, address))
        })
        .unzip();

    let nonce_values = storage
        .storage_web3_dal()
        .get_values(&nonce_keys)
        .await
        .context("failed getting nonces from storage")?;

    Ok(nonce_values
        .into_iter()
        .map(|(nonce_key, nonce_value)| {
            // `unwrap()` is safe by construction.
            let be_u32_bytes: [u8; 4] = nonce_value[28..].try_into().unwrap();
            let nonce = Nonce(u32::from_be_bytes(be_u32_bytes));
            (address_by_nonce_key[&nonce_key], nonce)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
    use zksync_node_fee_model::MockBatchFeeParamsProvider;
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_node_test_utils::create_l2_transaction;
    use zksync_types::{
        u256_to_h256, L2BlockNumber, PriorityOpId, ProtocolVersionId, StorageLog, H256,
    };

    use super::*;

    const TEST_MEMPOOL_CONFIG: MempoolConfig = MempoolConfig {
        sync_interval_ms: 10,
        sync_batch_size: 100,
        capacity: 100,
        stuck_tx_timeout: 0,
        remove_stuck_txs: false,
        delay_interval: 10,
    };

    #[tokio::test]
    async fn getting_transaction_nonces() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let transaction = create_l2_transaction(10, 100);
        let transaction_initiator = transaction.initiator_account();
        let nonce_key = get_nonce_key(&transaction_initiator);
        let nonce_log = StorageLog::new_write_log(nonce_key, u256_to_h256(42.into()));
        storage
            .storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(0), &[nonce_log])
            .await
            .unwrap();

        let other_transaction = create_l2_transaction(10, 100);
        let other_transaction_initiator = other_transaction.initiator_account();
        assert_ne!(other_transaction_initiator, transaction_initiator);

        let nonces = get_transaction_nonces(
            &mut storage,
            &[&transaction.into(), &other_transaction.into()],
        )
        .await
        .unwrap();
        assert_eq!(
            nonces,
            HashMap::from([
                (transaction_initiator, Nonce(42)),
                (other_transaction_initiator, Nonce(0)),
            ])
        );
    }

    #[tokio::test]
    async fn syncing_mempool_basics() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        drop(storage);

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
        let fee_params_provider: Arc<dyn BatchFeeModelInputProvider> =
            Arc::new(MockBatchFeeParamsProvider::default());
        let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());

        let mut fetcher = MempoolFetcher::new(
            mempool.clone(),
            fee_params_provider,
            &TEST_MEMPOOL_CONFIG,
            pool.clone(),
        );
        let (tx_hashes_sender, mut tx_hashes_receiver) = mpsc::unbounded_channel();
        fetcher.transaction_hashes_sender = tx_hashes_sender;
        let (stop_sender, stop_receiver) = watch::channel(false);
        let fetcher_task = tokio::spawn(fetcher.run(stop_receiver));

        // Add a new transaction to the storage.
        let transaction = create_l2_transaction(base_fee, gas_per_pubdata);
        let transaction_hash = transaction.hash();
        let mut storage = pool.connection().await.unwrap();
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &transaction,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        drop(storage);

        // Check that the transaction is eventually synced.
        let tx_hashes = wait_for_new_transactions(&mut tx_hashes_receiver).await;
        assert_eq!(tx_hashes, [transaction_hash]);
        assert_eq!(mempool.stats().l2_transaction_count, 1);

        stop_sender.send_replace(true);
        fetcher_task.await.unwrap().expect("fetcher errored");
    }

    async fn wait_for_new_transactions(
        tx_hashes_receiver: &mut mpsc::UnboundedReceiver<Vec<H256>>,
    ) -> Vec<H256> {
        loop {
            let tx_hashes = tx_hashes_receiver.recv().await.unwrap();
            if tx_hashes.is_empty() {
                continue;
            }
            break tx_hashes;
        }
    }

    #[tokio::test]
    async fn ignoring_transaction_with_insufficient_fee() {
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        drop(storage);

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
        let fee_params_provider: Arc<dyn BatchFeeModelInputProvider> =
            Arc::new(MockBatchFeeParamsProvider::default());
        let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());

        let fetcher = MempoolFetcher::new(
            mempool.clone(),
            fee_params_provider,
            &TEST_MEMPOOL_CONFIG,
            pool.clone(),
        );
        let (stop_sender, stop_receiver) = watch::channel(false);
        let fetcher_task = tokio::spawn(fetcher.run(stop_receiver));

        // Add a transaction with insufficient fee to the storage.
        let transaction = create_l2_transaction(base_fee / 2, gas_per_pubdata / 2);
        let mut storage = pool.connection().await.unwrap();
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &transaction,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        drop(storage);

        tokio::time::sleep(TEST_MEMPOOL_CONFIG.sync_interval() * 5).await;
        assert_eq!(mempool.stats().l2_transaction_count, 0);

        stop_sender.send_replace(true);
        fetcher_task.await.unwrap().expect("fetcher errored");
    }

    #[tokio::test]
    async fn ignoring_transaction_with_old_nonce() {
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        drop(storage);

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
        let fee_params_provider: Arc<dyn BatchFeeModelInputProvider> =
            Arc::new(MockBatchFeeParamsProvider::default());
        let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());

        let mut fetcher = MempoolFetcher::new(
            mempool.clone(),
            fee_params_provider,
            &TEST_MEMPOOL_CONFIG,
            pool.clone(),
        );
        let (tx_hashes_sender, mut tx_hashes_receiver) = mpsc::unbounded_channel();
        fetcher.transaction_hashes_sender = tx_hashes_sender;
        let (stop_sender, stop_receiver) = watch::channel(false);
        let fetcher_task = tokio::spawn(fetcher.run(stop_receiver));

        // Add a new transaction to the storage.
        let transaction = create_l2_transaction(base_fee * 2, gas_per_pubdata * 2);
        assert_eq!(transaction.nonce(), Nonce(0));
        let transaction_hash = transaction.hash();
        let nonce_key = get_nonce_key(&transaction.initiator_account());
        let nonce_log = StorageLog::new_write_log(nonce_key, u256_to_h256(42.into()));
        let mut storage = pool.connection().await.unwrap();
        storage
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &[nonce_log])
            .await
            .unwrap();
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &transaction,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        drop(storage);

        // Check that the transaction is eventually synced.
        let tx_hashes = wait_for_new_transactions(&mut tx_hashes_receiver).await;
        assert_eq!(tx_hashes, [transaction_hash]);
        // Transaction must not be added to the pool because of its outdated nonce.
        assert_eq!(mempool.stats().l2_transaction_count, 0);

        stop_sender.send_replace(true);
        fetcher_task.await.unwrap().expect("fetcher errored");
    }
}
