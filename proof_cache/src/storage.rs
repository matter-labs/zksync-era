use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;
use zksync_airbender_execution_utils::ProgramProof;

#[async_trait::async_trait]
pub trait Storage<K, V>: Debug + Send + Sync + 'static {
    async fn get(&self, key: &K) -> Option<V>;
    async fn put(&self, key: K, value: V);
}

pub type DynStorage = Arc<dyn Storage<String, ProgramProof>>;

#[derive(Debug)]
pub struct MemoryStorage {
    mapping: Mutex<HashMap<String, (ProgramProof, DateTime<Utc>)>>,
    // TODO: Implement later
    ttl: chrono::Duration,
}

impl MemoryStorage {
    pub fn new() -> Self {
        MemoryStorage {
            mapping: Mutex::new(HashMap::new()),
            ttl: chrono::Duration::hours(24),
        }
    }

    pub async fn evict_expired(&self) {
        let mut mapping = self.mapping.lock().await;
        mapping.retain(|_, (_, timestamp)| Utc::now().signed_duration_since(*timestamp) < self.ttl);
    }
}

#[async_trait::async_trait]
impl Storage<String, ProgramProof> for MemoryStorage {
    async fn get(&self, key: &String) -> Option<ProgramProof> {
        let mapping = self.mapping.lock().await;
        mapping.get(key).map(|(value, _)| value.clone())
    }

    async fn put(&self, key: String, value: ProgramProof) {
        let mut mapping = self.mapping.lock().await;
        if mapping.contains_key(&key) {
            tracing::warn!("Proof already exists in storage: {}", key);
            return;
        };
        // TODO: validate proof here, or maybe in handler?
        mapping.insert(key, (value, Utc::now()));
    }
}