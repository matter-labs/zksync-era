use tokio::runtime::Handle;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_state::PostgresStorage;
use zksync_types::{
    api::state_override::StateOverride, AccountTreeId, L2BlockNumber, StorageKey, StorageLog, H256,
};

use super::storage::apply_state_override;

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
