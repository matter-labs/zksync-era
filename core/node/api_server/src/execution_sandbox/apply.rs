use std::time::{Duration, Instant};

use anyhow::Context as _;
use tokio::runtime::Handle;
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::OneshotEnv;
use zksync_state::{PostgresStorage, PostgresStorageCaches};
use zksync_vm_executor::oneshot::TxSetupArgs;

use super::{
    vm_metrics::{SandboxStage, SANDBOX_METRICS},
    BlockArgs,
};

// FIXME: pass args by ref
pub(super) async fn prepare_env_and_storage(
    mut connection: Connection<'static, Core>,
    setup_args: TxSetupArgs,
    block_args: &BlockArgs,
    storage_caches: Option<PostgresStorageCaches>,
) -> anyhow::Result<(OneshotEnv, PostgresStorage<'static>)> {
    let initialization_stage = SANDBOX_METRICS.sandbox[&SandboxStage::Initialization].start();
    let resolve_started_at = Instant::now();
    let resolve_time = resolve_started_at.elapsed();
    let resolved_block_info = block_args.inner.resolve(&mut connection).await?;
    // We don't want to emit too many logs.
    if resolve_time > Duration::from_millis(10) {
        tracing::debug!("Resolved block numbers (took {resolve_time:?})");
    }

    let env = setup_args
        .to_env(&mut connection, &resolved_block_info)
        .await?;

    if block_args.resolves_to_latest_sealed_l2_block() {
        if let Some(caches) = &storage_caches {
            caches.schedule_values_update(resolved_block_info.state_l2_block_number());
        }
    }

    let mut storage = PostgresStorage::new_async(
        Handle::current(),
        connection,
        resolved_block_info.state_l2_block_number(),
        false,
    )
    .await
    .context("cannot create `PostgresStorage`")?;

    if let Some(caches) = storage_caches {
        storage = storage.with_caches(caches);
    }
    initialization_stage.observe();
    Ok((env, storage))
}
