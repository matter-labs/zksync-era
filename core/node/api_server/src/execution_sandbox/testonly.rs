use std::{thread, time::Duration};

use async_trait::async_trait;
use tokio::runtime::Handle;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_multivm::interface::{
    storage::{ReadStorage, StorageWithOverrides},
    tracer::{ValidationError, ValidationParams, ValidationTraces},
    OneshotEnv, OneshotTracingParams, TxExecutionArgs,
};
use zksync_state::PostgresStorage;
use zksync_types::{
    api::state_override::StateOverride, l2::L2Tx, AccountTreeId, L2BlockNumber, StorageKey,
    StorageLog, StorageValue, H256,
};
use zksync_vm_executor::{interface::TransactionValidator, oneshot::MainOneshotExecutor};

use super::storage::apply_state_override;
use crate::execution_sandbox::{
    execute::{SandboxExecutorEngine, SandboxStorage},
    SandboxExecutionOutput,
};

/// Applies overrides to the Postgres storage by inserting the necessary storage logs / factory deps into the genesis block.
pub(crate) async fn apply_state_overrides(
    mut connection: Connection<'static, Core>,
    state_override: StateOverride,
) {
    let latest_block = connection
        .blocks_dal()
        .get_sealed_l2_block_number()
        .await
        .unwrap()
        .expect("no blocks in Postgres");
    let state = PostgresStorage::new_async(Handle::current(), connection, latest_block, false)
        .await
        .unwrap();
    let state_with_overrides =
        tokio::task::spawn_blocking(|| apply_state_override(state, state_override))
            .await
            .unwrap();
    let (state, overrides) = state_with_overrides.into_parts();

    let mut connection = state.into_inner();
    let mut storage_logs = vec![];
    // Old logs must be erased before inserting `overridden_slots`.
    let all_existing_logs = connection
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    for log in all_existing_logs {
        if let (Some(addr), Some(key)) = (log.address, log.key) {
            let account = AccountTreeId::new(addr);
            if overrides.empty_accounts.contains(&account) {
                let key = StorageKey::new(account, key);
                storage_logs.push(StorageLog::new_write_log(key, H256::zero()));
            }
        }
    }

    storage_logs.extend(
        overrides
            .overridden_slots
            .into_iter()
            .map(|(key, value)| StorageLog::new_write_log(key, value)),
    );

    connection
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &storage_logs)
        .await
        .unwrap();
    connection
        .factory_deps_dal()
        .insert_factory_deps(L2BlockNumber(0), &overrides.overridden_factory_deps)
        .await
        .unwrap();
}

#[derive(Debug)]
struct SlowStorage<S> {
    inner: S,
    delay: Duration,
}

impl<S: ReadStorage> SlowStorage<S> {
    fn new(inner: S, delay: Duration) -> Self {
        Self { inner, delay }
    }
}

impl<S: ReadStorage> ReadStorage for SlowStorage<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        thread::sleep(self.delay);
        self.inner.read_value(key)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        thread::sleep(self.delay);
        self.inner.is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        thread::sleep(self.delay);
        self.inner.load_factory_dep(hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.inner.get_enumeration_index(key)
    }
}

/// Executor that artificially slows down storage accesses.
#[derive(Debug)]
pub(crate) struct SlowExecutor {
    inner: MainOneshotExecutor,
    delay: Duration,
}

impl SlowExecutor {
    pub fn new(inner: MainOneshotExecutor, delay: Duration) -> Self {
        Self { inner, delay }
    }

    fn patch_storage(
        &self,
        storage: SandboxStorage,
    ) -> StorageWithOverrides<SlowStorage<PostgresStorage<'static>>> {
        let (storage, overrides) = storage.into_parts();
        let storage = SlowStorage::new(storage, self.delay);
        StorageWithOverrides::new(storage).with_overrides(overrides)
    }
}

#[async_trait]
impl TransactionValidator<SandboxStorage> for SlowExecutor {
    async fn validate_transaction(
        &self,
        storage: SandboxStorage,
        env: OneshotEnv,
        tx: L2Tx,
        validation_params: ValidationParams,
    ) -> anyhow::Result<Result<ValidationTraces, ValidationError>> {
        let storage = self.patch_storage(storage);
        self.inner
            .validate_transaction(storage, env, tx, validation_params)
            .await
    }
}

#[async_trait]
impl SandboxExecutorEngine for SlowExecutor {
    async fn execute_in_sandbox(
        &self,
        storage: SandboxStorage,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<SandboxExecutionOutput> {
        let storage = self.patch_storage(storage);
        self.inner
            .execute_in_sandbox(storage, env, args, tracing_params)
            .await
    }
}
