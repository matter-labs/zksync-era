use std::cmp::Ordering;

use anyhow::Context;
use zksync_basic_types::{L1BatchNumber, MiniblockNumber};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::ProtocolVersionId;
use zksync_web3_decl::{
    client::L2Client,
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
};

pub async fn get_l1_batch_remote_protocol_version(
    main_node_client: &L2Client,
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<Option<ProtocolVersionId>> {
    let Some((miniblock, _)) = main_node_client
        .get_miniblock_range(l1_batch_number)
        .await?
    else {
        return Ok(None);
    };
    let sync_block = main_node_client
        .sync_l2_block(MiniblockNumber(miniblock.as_u32()), false)
        .await?;
    Ok(sync_block.map(|b| b.protocol_version))
}

// Synchronizes protocol version in `l1_batches` and `miniblocks` tables between EN and main node.
pub async fn sync_versions(
    connection_pool: ConnectionPool<Core>,
    main_node_client: L2Client,
) -> anyhow::Result<()> {
    tracing::info!("Starting syncing protocol version of blocks");

    let mut connection = connection_pool.connection().await?;

    // Load the first local batch number with version 22.
    let Some(local_first_v22_l1_batch) = connection
        .blocks_dal()
        .get_first_l1_batch_number_for_version(ProtocolVersionId::Version22)
        .await?
    else {
        return Ok(());
    };
    tracing::info!("First local v22 batch is #{local_first_v22_l1_batch}");

    // Find the first remote batch with version 22, assuming it's less than or equal than local one.
    // Uses binary search.
    let mut left_bound = L1BatchNumber(0);
    let mut right_bound = local_first_v22_l1_batch;

    let right_bound_remote_version =
        get_l1_batch_remote_protocol_version(&main_node_client, right_bound).await?;
    if right_bound_remote_version != Some(ProtocolVersionId::Version22) {
        anyhow::bail!("Remote protocol versions should be v22 for the first local v22 batch, got {right_bound_remote_version:?}");
    }

    while left_bound < right_bound {
        let mid_batch = L1BatchNumber((left_bound.0 + right_bound.0) / 2);
        let (mid_miniblock, _) = connection
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(mid_batch)
            .await?
            .with_context(|| {
                format!("Postgres is inconsistent: missing miniblocks for L1 batch #{mid_batch}")
            })?;
        let mid_protocol_version = main_node_client
            .sync_l2_block(mid_miniblock, false)
            .await?
            .with_context(|| format!("Main node missing data about miniblock #{mid_miniblock}"))?
            .protocol_version;

        match mid_protocol_version.cmp(&ProtocolVersionId::Version22) {
            Ordering::Less => {
                left_bound = mid_batch + 1;
            }
            Ordering::Equal => {
                right_bound = mid_batch;
            }
            Ordering::Greater => {
                anyhow::bail!("Unexpected remote protocol version: {mid_protocol_version:?} for miniblock #{mid_miniblock}");
            }
        }
    }

    let remote_first_v22_l1_batch = left_bound;
    let (remote_first_v22_miniblock, _) = connection
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(remote_first_v22_l1_batch)
        .await?
        .with_context(|| {
            format!("Postgres is inconsistent: missing miniblocks for L1 batch #{remote_first_v22_l1_batch}")
        })?;

    let mut transaction = connection.start_transaction().await?;

    tracing::info!(
        "Setting version 22 for batches {remote_first_v22_l1_batch}..={local_first_v22_l1_batch}"
    );
    transaction
        .blocks_dal()
        .reset_protocol_version_for_l1_batches(
            remote_first_v22_l1_batch..=local_first_v22_l1_batch,
            ProtocolVersionId::Version22,
        )
        .await?;

    let (local_first_v22_miniblock, _) = transaction
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(local_first_v22_l1_batch)
        .await?
        .with_context(|| {
            format!("Postgres is inconsistent: missing miniblocks for L1 batch #{local_first_v22_l1_batch}")
        })?;

    tracing::info!("Setting version 22 for miniblocks {remote_first_v22_miniblock}..={local_first_v22_miniblock}");
    transaction
        .blocks_dal()
        .reset_protocol_version_for_miniblocks(
            remote_first_v22_miniblock..=local_first_v22_miniblock,
            ProtocolVersionId::Version22,
        )
        .await?;

    transaction.commit().await?;

    Ok(())
}
