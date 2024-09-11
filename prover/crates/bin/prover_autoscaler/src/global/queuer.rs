use std::collections::HashMap;

// TODO use zksync_types::protocol_version::ProtocolSemanticVersion;

#[derive(Debug)]
pub struct ProverGroupQueue {
    queue: HashMap<u8, usize>,
}

#[derive(Debug)]
pub struct Queue {
    queue: HashMap<String, ProverGroupQueue>,
}

pub struct Queuer {}

impl Queuer {
    pub async fn get_queue(&self) -> anyhow::Result<Queue> {
        // TODO: update prover job monitor to provide endpoint to query queue.
        Ok(Queue {
            queue: HashMap::from([(
                "0.24.2".to_string(),
                ProverGroupQueue {
                    queue: HashMap::from([(0, 1000), (0, 99999)]),
                },
            )]),
        })
    }
}
