use vm2::Event;
use zksync_types::{L1BatchNumber, H256};
use zksync_utils::h256_to_account_address;

use crate::interface::VmEvent;

#[derive(Clone)]
struct EventAccumulator {
    pub(crate) shard_id: u8,
    pub(crate) tx_number_in_block: u16,
    pub(crate) topics: Vec<[u8; 32]>,
    pub(crate) data: Vec<u8>,
}

impl EventAccumulator {
    fn into_vm_event(self, block_number: L1BatchNumber) -> VmEvent {
        VmEvent {
            location: (block_number, self.tx_number_in_block as u32),
            address: h256_to_account_address(&H256(self.topics[0])),
            indexed_topics: self.topics[1..].iter().map(H256::from).collect(),
            value: self.data,
        }
    }
}

pub(crate) fn merge_events(events: &[Event], block_number: L1BatchNumber) -> Vec<VmEvent> {
    let mut result = vec![];
    let mut current: Option<(usize, u32, EventAccumulator)> = None;

    for message in events.iter() {
        let Event {
            shard_id,
            is_first,
            tx_number,
            key,
            value,
        } = message.clone();

        if !is_first {
            if let Some((mut remaining_data_length, mut remaining_topics, mut event)) =
                current.take()
            {
                if event.shard_id != shard_id || event.tx_number_in_block != tx_number {
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
                    result.push(event.into_vm_event(block_number));
                }
            }
        } else {
            // start new one. First take the old one only if it's well formed
            if let Some((remaining_data_length, remaining_topics, event)) = current.take() {
                if remaining_data_length == 0 && remaining_topics == 0 {
                    result.push(event.into_vm_event(block_number));
                }
            }

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

            let new_event = EventAccumulator {
                shard_id,
                tx_number_in_block: tx_number,
                topics,
                data,
            };

            current = Some((data_length, num_topics, new_event))
        }
    }

    // add the last one
    if let Some((remaining_data_length, remaining_topics, event)) = current.take() {
        if remaining_data_length == 0 && remaining_topics == 0 {
            result.push(event.into_vm_event(block_number));
        }
    }

    result
}
