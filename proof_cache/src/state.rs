use crate::storage::DynStorage;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use zksync_airbender_execution_utils::ProgramProof;

#[derive(Clone, Serialize, Deserialize)]
pub struct LinkedProof {
    pub start_block: u32,
    pub end_block_inclusive: u32,
    pub proof: ProgramProof,
}

impl Debug for LinkedProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkedProof")
            .field("start_block", &self.start_block)
            .field("end_block_inclusive", &self.end_block_inclusive)
            .field("proof", &"<PROOF>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    fri_storage: DynStorage,
    linked_storage: Arc<Mutex<Option<LinkedProof>>>,
    // TODO: add later snark_storage
}

impl AppState {
    pub fn new(fri_storage: DynStorage) -> Self {
        AppState {
            fri_storage,
            linked_storage: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_fri_proof(&self, key: &String) -> Option<ProgramProof> {
        self.fri_storage.get(key).await
    }

    pub async fn put_fri_proof(&self, key: String, value: ProgramProof) {
        self.fri_storage.put(key, value).await;
    }

    pub async fn get_linked_proof(&self) -> Option<LinkedProof> {
        let linked_storage = self.linked_storage.lock().expect("mutex poisoned, failed to lock linked proof");
        linked_storage.clone()
    }

    pub async fn put_linked_proof(&self, linked_proof: LinkedProof) {
        let mut linked_storage = self.linked_storage.lock().expect("mutex poisoned, failed to lock linked proof");
        // TODO: do some magic validation here to make sure we're not overwriting a longer or "better" (to be defined what that means) linked proof
        *linked_storage = Some(linked_proof);
    }
}