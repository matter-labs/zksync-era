//! Utils for commitment calculation.

use std::fmt;

use anyhow::Context;
use itertools::Itertools;
use zk_evm_1_5_2::{
    aux_structures::Timestamp as Timestamp_1_5_2,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_5_2,
};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::commit::kzg::ZK_SYNC_BYTES_PER_BLOB;
use zksync_multivm::{interface::VmEvent, utils::get_used_bootloader_memory_bytes};
use zksync_system_constants::message_root::{AGG_TREE_HEIGHT_KEY, AGG_TREE_NODES_KEY};
use zksync_types::{
    address_to_u256, h256_to_u256, u256_to_h256,
    web3::keccak256,
    zk_evm_types::{LogQuery, Timestamp},
    AccountTreeId, L1BatchNumber, ProtocolVersionId, StorageKey, EVENT_WRITER_ADDRESS, H256,
    L2_MESSAGE_ROOT_ADDRESS, U256,
};

/// Encapsulates computations of commitment components.
///
/// - All methods are considered to be blocking.
/// - Returned errors are considered unrecoverable (i.e., they bubble up and lead to commitment generator termination).
pub(crate) trait CommitmentComputer: fmt::Debug + Send + Sync + 'static {
    fn events_queue_commitment(
        &self,
        events_queue: &[LogQuery],
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256>;

    fn bootloader_initial_content_commitment(
        &self,
        initial_bootloader_contents: &[(usize, U256)],
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256>;
}

#[derive(Debug)]
pub(crate) struct RealCommitmentComputer;

impl CommitmentComputer for RealCommitmentComputer {
    fn events_queue_commitment(
        &self,
        events_queue: &[LogQuery],
        _protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256> {
        // FIXME: is this OK to process using 0.151.x?
        let commitment = circuit_encodings::commitments::events_queue_commitment_fixed(
            &events_queue
                .iter()
                .map(|x| to_log_query_1_5_2(*x))
                .collect(),
        );
        Ok(H256(commitment))
    }

    fn bootloader_initial_content_commitment(
        &self,
        initial_bootloader_contents: &[(usize, U256)],
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256> {
        let expanded_memory_size = if protocol_version.is_pre_boojum() {
            anyhow::bail!("Unsupported protocol version: {protocol_version:?}");
        } else {
            get_used_bootloader_memory_bytes(protocol_version.into())
        };

        let full_bootloader_memory =
            expand_memory_contents(initial_bootloader_contents, expanded_memory_size);
        let commitment = circuit_encodings::commitments::initial_heap_content_commitment_fixed(
            &full_bootloader_memory,
        );
        Ok(H256(commitment))
    }
}

fn expand_memory_contents(packed: &[(usize, U256)], memory_size_bytes: usize) -> Vec<u8> {
    let mut result: Vec<u8> = vec![0; memory_size_bytes];

    for (offset, value) in packed {
        value.to_big_endian(&mut result[(offset * 32)..(offset + 1) * 32]);
    }

    result
}

fn to_log_query_1_5_2(log_query: LogQuery) -> LogQuery_1_5_2 {
    LogQuery_1_5_2 {
        timestamp: Timestamp_1_5_2(log_query.timestamp.0),
        tx_number_in_block: log_query.tx_number_in_block,
        aux_byte: log_query.aux_byte,
        shard_id: log_query.shard_id,
        address: log_query.address,
        key: log_query.key,
        read_value: log_query.read_value,
        written_value: log_query.written_value,
        rw_flag: log_query.rw_flag,
        rollback: log_query.rollback,
        is_service: log_query.is_service,
    }
}

/// Each `VmEvent` can be translated to several log queries.
/// This methods converts each event from input to log queries and returns all produced log queries.
pub(crate) fn convert_vm_events_to_log_queries(events: &[VmEvent]) -> Vec<LogQuery> {
    events
        .iter()
        .flat_map(|event| {
            // Construct first query. This query holds an information about
            // - number of event topics (on log query level `event.address` is treated as a topic, thus + 1 is added)
            // - length of event value
            // - `event.address` (or first topic in terms of log query terminology).
            let first_key_word =
                (event.indexed_topics.len() as u64 + 1) + ((event.value.len() as u64) << 32);
            let key = U256([first_key_word, 0, 0, 0]);

            // `timestamp`, `aux_byte`, `read_value`, `rw_flag`, `rollback` are set as per convention.
            let first_log = LogQuery {
                timestamp: Timestamp(0),
                tx_number_in_block: event.location.1 as u16,
                aux_byte: 0,
                shard_id: 0,
                address: EVENT_WRITER_ADDRESS,
                key,
                read_value: U256::zero(),
                written_value: address_to_u256(&event.address),
                rw_flag: false,
                rollback: false,
                is_service: true,
            };

            // The next logs hold information about remaining topics and `event.value`.
            // Each log can hold at most two values each of 32 bytes.
            // The following piece of code prepares these 32-byte values.
            let values = event.indexed_topics.iter().map(|h| h256_to_u256(*h)).chain(
                event.value.chunks(32).map(|value_chunk| {
                    let mut padded = value_chunk.to_vec();
                    padded.resize(32, 0);
                    U256::from_big_endian(&padded)
                }),
            );

            // And now we process these values in chunks by two.
            let value_chunks = values.chunks(2);
            let other_logs = value_chunks.into_iter().map(|mut chunk| {
                // The first value goes to `log_query.key`.
                let key = chunk.next().unwrap();

                // If the second one is present then it goes to `log_query.written_value`.
                let written_value = chunk.next().unwrap_or_default();

                LogQuery {
                    timestamp: Timestamp(0),
                    tx_number_in_block: event.location.1 as u16,
                    aux_byte: 0,
                    shard_id: 0,
                    address: EVENT_WRITER_ADDRESS,
                    key,
                    read_value: U256::zero(),
                    written_value,
                    rw_flag: false,
                    rollback: false,
                    is_service: false,
                }
            });

            std::iter::once(first_log)
                .chain(other_logs)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(crate) fn pubdata_to_blob_linear_hashes(
    blobs_required: usize,
    mut pubdata_input: Vec<u8>,
) -> Vec<H256> {
    // Now, we need to calculate the linear hashes of the blobs.
    // Firstly, let's pad the pubdata to the size of the blob.
    if pubdata_input.len() % ZK_SYNC_BYTES_PER_BLOB != 0 {
        pubdata_input.resize(
            pubdata_input.len()
                + (ZK_SYNC_BYTES_PER_BLOB - pubdata_input.len() % ZK_SYNC_BYTES_PER_BLOB),
            0,
        );
    }

    let mut result = vec![H256::zero(); blobs_required];

    pubdata_input
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .enumerate()
        .for_each(|(i, chunk)| {
            result[i] = H256(keccak256(chunk));
        });

    result
}

pub(crate) async fn read_aggregation_root(
    connection: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<H256> {
    let (_, last_l2_block) = connection
        .blocks_dal()
        .get_l2_block_range_of_l1_batch(l1_batch_number)
        .await?
        .context("No range for batch")?;

    let agg_tree_height_slot = StorageKey::new(
        AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS),
        H256::from_low_u64_be(AGG_TREE_HEIGHT_KEY as u64),
    );

    let agg_tree_height = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(agg_tree_height_slot.hashed_key(), last_l2_block)
        .await?;
    let agg_tree_height = h256_to_u256(agg_tree_height);

    // `nodes[height][0]`
    let agg_tree_root_hash_key =
        n_dim_array_key_in_layout(AGG_TREE_NODES_KEY, &[agg_tree_height, U256::zero()]);
    let agg_tree_root_hash_slot = StorageKey::new(
        AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS),
        agg_tree_root_hash_key,
    );

    Ok(connection
        .storage_web3_dal()
        .get_historical_value_unchecked(agg_tree_root_hash_slot.hashed_key(), last_l2_block)
        .await?)
}

fn n_dim_array_key_in_layout(array_key: usize, indices: &[U256]) -> H256 {
    let mut key: H256 = u256_to_h256(array_key.into());

    for index in indices {
        key = H256(keccak256(key.as_bytes()));
        key = u256_to_h256(h256_to_u256(key).overflowing_add(*index).0);
    }

    key
}
