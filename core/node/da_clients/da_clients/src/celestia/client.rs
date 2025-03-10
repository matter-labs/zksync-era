use std::sync::Mutex;

pub struct CelestiaClient {
    // ... other fields ...
    equivalence_proof_cache: Mutex<HashMap<String, (FixedBytes<32>, FixedBytes<32>, SP1ProofWithPublicValues)>>,
}

impl CelestiaClient {
    pub async fn new(/* ... */) -> anyhow::Result<Self> {
        Ok(Self {
            // ... other fields ...
            equivalence_proof_cache: Mutex::new(HashMap::new()),
        })
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let (keccak_hash, data_root, proof) = match self.equivalence_proof_cache.lock().unwrap().get(&blob_id.to_string()) {
            Some(cached_proof) => {
                tracing::debug!("Found cached proof for blob_id: {}", blob_id);
                (cached_proof.0, cached_proof.1, cached_proof.2.clone())
            },
            None => {
                match self.get_proof_data(&blob_id).await? {
                    Some(proof) => {
                        tracing::debug!("Got complete zk equivallence proof for blob_id: {}", blob_id);
                        self.equivalence_proof_cache.lock().unwrap().insert(blob_id.to_string(), proof.clone());
                        proof
                    },
                    None => return Ok(None),
                }
            }
        };

        let (from, to, proof_nonce) = find_block_range(
            &self.eth_client,
            target_height,
            latest_block,
            BlockNumber::Number(block_num),
            &self.blobstream_update_event,
            &self.config.blobstream_contract_address,
        ).await.expect("Failed to find block range"); 
    }
} 