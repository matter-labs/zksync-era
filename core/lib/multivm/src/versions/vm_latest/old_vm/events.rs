use zk_evm_1_5_2::{ethereum_types::Address, reference_impls::event_sink::EventMessage};
use zksync_types::{h256_to_address, L1BatchNumber, EVENT_WRITER_ADDRESS, H256};

use crate::{interface::VmEvent, utils::bytecode::be_chunks_to_h256_words};

#[derive(Clone)]
pub(crate) struct SolidityLikeEvent {
    pub(crate) shard_id: u8,
    pub(crate) tx_number_in_block: u16,
    pub(crate) address: Address,
    pub(crate) topics: Vec<[u8; 32]>,
    pub(crate) data: Vec<u8>,
}

impl SolidityLikeEvent {
    pub(crate) fn into_vm_event(self, block_number: L1BatchNumber) -> VmEvent {
        VmEvent {
            location: (block_number, self.tx_number_in_block as u32),
            address: self.address,
            indexed_topics: be_chunks_to_h256_words(self.topics),
            value: self.data,
        }
    }
}

fn merge_events_inner(events: Vec<EventMessage>) -> Vec<SolidityLikeEvent> {
    let mut result = vec![];
    let mut current: Option<(usize, u32, SolidityLikeEvent)> = None;

    for message in events.into_iter() {
        if !message.is_first {
            let EventMessage {
                shard_id,
                is_first: _,
                tx_number_in_block,
                address,
                key,
                value,
            } = message;

            if let Some((mut remaining_data_length, mut remaining_topics, mut event)) =
                current.take()
            {
                if event.address != address
                    || event.shard_id != shard_id
                    || event.tx_number_in_block != tx_number_in_block
                {
                    continue;
                }
                let mut data_0 = [0u8; 32];
                let mut data_1 = [0u8; 32];
                key.to_big_endian(&mut data_0);
                value.to_big_endian(&mut data_1);
                for el in [data_0, data_1].iter() {
                    if remaining_topics != 0 {
                        event.topics.push(*el);
                        remaining_topics -= 1;
                    } else if remaining_data_length != 0 {
                        if remaining_data_length >= 32 {
                            event.data.extend_from_slice(el);
                            remaining_data_length -= 32;
                        } else {
                            event.data.extend_from_slice(&el[..remaining_data_length]);
                            remaining_data_length = 0;
                        }
                    }
                }

                if remaining_data_length != 0 || remaining_topics != 0 {
                    current = Some((remaining_data_length, remaining_topics, event))
                } else {
                    result.push(event);
                }
            }
        } else {
            // start new one. First take the old one only if it's well formed
            if let Some((remaining_data_length, remaining_topics, event)) = current.take() {
                if remaining_data_length == 0 && remaining_topics == 0 {
                    result.push(event);
                }
            }

            let EventMessage {
                shard_id,
                is_first: _,
                tx_number_in_block,
                address,
                key,
                value,
            } = message;
            // split key as our internal marker. Ignore higher bits
            let mut num_topics = key.0[0] as u32;
            let mut data_length = (key.0[0] >> 32) as usize;
            let mut buffer = [0u8; 32];
            value.to_big_endian(&mut buffer);

            let (topics, data) = if num_topics == 0 && data_length == 0 {
                (vec![], vec![])
            } else if num_topics == 0 {
                data_length -= 32;
                (vec![], buffer.to_vec())
            } else {
                num_topics -= 1;
                (vec![buffer], vec![])
            };

            let new_event = SolidityLikeEvent {
                shard_id,
                tx_number_in_block,
                address,
                topics,
                data,
            };

            current = Some((data_length, num_topics, new_event))
        }
    }

    // add the last one
    if let Some((remaining_data_length, remaining_topics, event)) = current.take() {
        if remaining_data_length == 0 && remaining_topics == 0 {
            result.push(event);
        }
    }

    result
}

pub(crate) fn merge_events(events: Vec<EventMessage>) -> Vec<SolidityLikeEvent> {
    let raw_events = merge_events_inner(events);

    raw_events
        .into_iter()
        .filter(|e| e.address == EVENT_WRITER_ADDRESS)
        .map(|event| {
            // The events writer events where the first topic is the actual address of the event and the rest of the topics are real topics
            let address = h256_to_address(&H256(event.topics[0]));
            let topics = event.topics.into_iter().skip(1).collect();

            SolidityLikeEvent {
                topics,
                address,
                ..event
            }
        })
        .collect()
}
