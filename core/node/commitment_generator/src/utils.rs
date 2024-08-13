//! Utils for commitment calculation.

use std::fmt;

use itertools::Itertools;
use zk_evm_1_3_3::{
    aux_structures::Timestamp as Timestamp_1_3_3,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_3_3,
};
use zk_evm_1_4_1::{
    aux_structures::Timestamp as Timestamp_1_4_1,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_4_1,
};
use zk_evm_1_5_0::{
    aux_structures::Timestamp as Timestamp_1_5_0,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_5_0,
};
use zksync_multivm::utils::get_used_bootloader_memory_bytes;
use zksync_types::{
    vm::VmVersion,
    zk_evm_types::{LogQuery, Timestamp},
    ProtocolVersionId, VmEvent, EVENT_WRITER_ADDRESS, H256, U256,
};
use zksync_utils::{address_to_u256, expand_memory_contents, h256_to_u256};

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
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<H256> {
        match VmVersion::from(protocol_version) {
            VmVersion::VmBoojumIntegration => Ok(H256(
                circuit_sequencer_api_1_4_0::commitments::events_queue_commitment_fixed(
                    &events_queue
                        .iter()
                        .map(|x| to_log_query_1_3_3(*x))
                        .collect(),
                ),
            )),
            VmVersion::Vm1_4_1 | VmVersion::Vm1_4_2 => Ok(H256(
                circuit_sequencer_api_1_4_1::commitments::events_queue_commitment_fixed(
                    &events_queue
                        .iter()
                        .map(|x| to_log_query_1_4_1(*x))
                        .collect(),
                ),
            )),
            VmVersion::Vm1_5_0SmallBootloaderMemory
            | VmVersion::Vm1_5_0IncreasedBootloaderMemory => Ok(H256(
                circuit_sequencer_api_1_5_0::commitments::events_queue_commitment_fixed(
                    &events_queue
                        .iter()
                        .map(|x| to_log_query_1_5_0(*x))
                        .collect(),
                ),
            )),
            _ => anyhow::bail!("Unsupported protocol version: {protocol_version:?}"),
        }
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

        match VmVersion::from(protocol_version) {
            VmVersion::VmBoojumIntegration => Ok(H256(
                circuit_sequencer_api_1_4_0::commitments::initial_heap_content_commitment_fixed(
                    &full_bootloader_memory,
                ),
            )),
            VmVersion::Vm1_4_1 | VmVersion::Vm1_4_2 => Ok(H256(
                circuit_sequencer_api_1_4_1::commitments::initial_heap_content_commitment_fixed(
                    &full_bootloader_memory,
                ),
            )),
            VmVersion::Vm1_5_0SmallBootloaderMemory
            | VmVersion::Vm1_5_0IncreasedBootloaderMemory => Ok(H256(
                circuit_sequencer_api_1_5_0::commitments::initial_heap_content_commitment_fixed(
                    &full_bootloader_memory,
                ),
            )),
            _ => unreachable!(),
        }
    }
}

fn to_log_query_1_3_3(log_query: LogQuery) -> LogQuery_1_3_3 {
    LogQuery_1_3_3 {
        timestamp: Timestamp_1_3_3(log_query.timestamp.0),
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

fn to_log_query_1_4_1(log_query: LogQuery) -> LogQuery_1_4_1 {
    LogQuery_1_4_1 {
        timestamp: Timestamp_1_4_1(log_query.timestamp.0),
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

fn to_log_query_1_5_0(log_query: LogQuery) -> LogQuery_1_5_0 {
    LogQuery_1_5_0 {
        timestamp: Timestamp_1_5_0(log_query.timestamp.0),
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
