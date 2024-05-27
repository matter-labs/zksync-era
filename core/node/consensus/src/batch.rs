

async fn snapshot(n: L1BatchNumber) -> anyhow::Result<Vec<StorageProof>> {
    let addr = constants::SYSTEM_CONTEXT_ADDRESS;
    let current_block_pos = constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION;
    let tx_rolling_hash_pos = constants::SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION;
    let resp = self.l2.get_proof(addr,vec![current_block_pos,tx_rolling_hash_pos],n).await.context("get_proof()")?.context("missing proof")?;
    let (block_number,_) = unpack_block_info(resp.storage_proof[0].value.as_bytes().into());
    let mut proof = resp.storage_proof;
    let prev_hash_pos =
        U256::from(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION.as_bytes()) +
        U256::from(block_number-1) % U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
    let resp = self.l2.get_proof(addr,vec![<[u8;32]>::from(prev_hash_pos).into()],n).await.context("get_proof()")?.context("missing proof")?;
    proof.extend(resp.storage_proof);
    Ok(proof)
}

async fn proof(n: L1BatchNumber) -> anyhow::Result<Vec<StorageProof>> {
    let mut proof = snapshot(n-1).await?;
    proof.extend(snapshot(n).await?);
    Ok(proof)
}

struct BatchProof {
    info: L1BatchWithMetadata, // we need a more compressed (intermediate) representation
    proof: Vec<StorageProof>,
}

impl BatchProof {
    fn verify(&self) -> anyhow::Result<()> {
        let token = StoredBatchInfo(&self.batch).into_token();
        let hash = H256(keccak256(&ethabi::encode(&[token])));
        // TODO: compare against L1
       
        // version in zksync_merkle_tree means l1 batch number
        // zksync_merkle_tree::types::TreeEntryWithProof::verify() 
        // it panics, so we need an error-returning version
        // Key is U256 (hashed key)
        // proof is a merkle path of length 256 with empty subtrees at the end skipped
    }
}

struct Batch {
    blocks: Vec<Payload>,
    before: BatchProof,
    after: BatchProof,
}

impl Batch {
    fn verify(&self) -> anyhow::Result<()> {
        self.before.verify()?;
        self.after.verify()?;
        anyhow::ensure!(self.before.info.number+1,self.after.info.number);
        // check that batch number of blocks matches self.after.info.number
        // check that exactly the last block of the batch is_last_block.
        // check that protocol_version l1_batch_number of the blocks is consistent
        // check that protocol_version is bigger than last-supported-protocol_version
        // compute verify the hash and timestamp of each block using the self.before and self.after as border
        //  conditions,
        // TODO:
        // are these only for sealing conditions: l1_gas_price, l2_fair_gas_price, fair_pubdata_price,
        // do these affect execution: virtual_blocks, operator_address 
    }
}
