use foundry_common::conversion_utils::h256_to_h160;
use multivm::zk_evm_1_4_0::reference_impls::event_sink::EventMessage;
use zksync_basic_types::{H160, H256};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LogEntry {
    pub address: H160,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
}

#[derive(Clone)]
struct SolidityLikeEvent {
    shard_id: u8,
    tx_number_in_block: u16,
    address: H160,
    topics: Vec<[u8; 32]>,
    data: Vec<u8>,
}

/// Ported from https://github.com/matter-labs/zksync-era/blob/0e2bc561b9642b854718adcc86087a3e9762cf5d/core/lib/multivm/src/versions/vm_latest/old_vm/events.rs
fn merge_events_inner(events: Vec<EventMessage>) -> Vec<SolidityLikeEvent> {
    let mut result = vec![];
    let mut current: Option<(usize, u32, SolidityLikeEvent)> = None;

    for message in events.into_iter() {
        if !message.is_first {
            let EventMessage { shard_id, is_first: _, tx_number_in_block, address, key, value } =
                message;

            if let Some((mut remaining_data_length, mut remaining_topics, mut event)) =
                current.take()
            {
                if event.address != address ||
                    event.shard_id != shard_id ||
                    event.tx_number_in_block != tx_number_in_block
                {
                    continue
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

            let EventMessage { shard_id, is_first: _, tx_number_in_block, address, key, value } =
                message;
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

            let new_event =
                SolidityLikeEvent { shard_id, tx_number_in_block, address, topics, data };

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

/// Parse a list of [EventMessage]s to [LogEntry]s.
pub fn parse_events(events: Vec<EventMessage>) -> Vec<LogEntry> {
    let raw_events = merge_events_inner(events);

    raw_events
        .into_iter()
        .map(|event| {
            // The events writer events where the first topic is the actual address of the event and
            // the rest of the topics are real topics
            LogEntry {
                address: h256_to_h160(&H256::from_slice(&event.topics[0])),
                topics: event
                    .topics
                    .into_iter()
                    .skip(1)
                    .map(|topic| H256::from_slice(&topic))
                    .collect(),
                data: event.data,
            }
        })
        .collect()
}
