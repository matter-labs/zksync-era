use zk_os_basic_system::system_implementation::system::BatchOutput;
use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata, ZkosCommitment},
    ethabi::{encode, Token},
    pubdata_da::PubdataSendingMode,
    L2ChainId,
};

use crate::{
    i_executor::{
        structures::{CommitBoojumOSBatchInfo, StoredBatchInfo, SUPPORTED_ENCODING_VERSION},
        zkos_commitment_to_vm_batch_output,
    },
    Tokenizable, Tokenize,
};

/// Input required to encode `commitBatches` call for a contract
#[derive(Debug)]
pub struct CommitBatches<'a> {
    pub last_committed_l1_batch: &'a L1BatchWithMetadata,
    pub l1_batches: &'a [L1BatchWithMetadata],
    pub pubdata_da: PubdataSendingMode,
    pub mode: L1BatchCommitmentMode,
    pub chain_id: L2ChainId,
}

impl Tokenize for &CommitBatches<'_> {
    fn into_tokens(self) -> Vec<Token> {
        // other modes are not yet supported in ZK OS
        assert_eq!(self.mode, L1BatchCommitmentMode::Rollup);
        assert_eq!(self.pubdata_da, PubdataSendingMode::Calldata);
        assert_eq!(
            self.l1_batches.len(),
            1,
            "Only one batch can be committed at a time in zk os"
        );
        let last_block_commitment: ZkosCommitment =
            ZkosCommitment::new(self.last_committed_l1_batch, self.chain_id);
        let batch_output: BatchOutput = zkos_commitment_to_vm_batch_output(&last_block_commitment);
        let stored_batch_info = StoredBatchInfo::new(&last_block_commitment, batch_output.hash());

        let l1_batch = self.l1_batches.first().unwrap();
        let current_block_commitment = ZkosCommitment::new(l1_batch, self.chain_id);

        let commit_boojum_os_batch_info = CommitBoojumOSBatchInfo::new(&current_block_commitment);

        tracing::info!(
            "Encoding CommitBatches previous batch ({}) - ZkosCommitment: {:?}",
            self.last_committed_l1_batch.header.number.0,
            last_block_commitment,
        );

        tracing::info!(
            "Encoding CommitBatches current batch ({}) - ZkosCommitment: {:?}",
            self.l1_batches[0].header.number.0,
            current_block_commitment,
        );

        tracing::info!(
            "Encoding StoredBatchInfo previous batch ({}) - StoredBatchInfo: {:?}",
            self.last_committed_l1_batch.header.number.0,
            stored_batch_info,
        );

        tracing::info!(
            "Encoding StoredBatchInfo previous batch ({}) - StoredBatchInfo hash: {:?}",
            self.last_committed_l1_batch.header.number.0,
            stored_batch_info.hash(),
        );

        tracing::info!(
            "Encoding CommitBatches for current batch ({}) - CommitBatchInfo: {:?}",
            self.l1_batches[0].header.number.0,
            commit_boojum_os_batch_info,
        );

        let mut encoded_data = encode(&[
            stored_batch_info.into_token(),
            commit_boojum_os_batch_info.into_token(),
        ]);

        encoded_data.insert(0, SUPPORTED_ENCODING_VERSION);
        vec![
            Token::Uint((self.last_committed_l1_batch.header.number.0 + 1).into()),
            Token::Uint(
                (self.last_committed_l1_batch.header.number.0 + self.l1_batches.len() as u32)
                    .into(),
            ),
            Token::Bytes(encoded_data),
        ]
    }
}
