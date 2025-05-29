use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi::{encode, Token},
    pubdata_da::PubdataSendingMode,
};

use crate::{
    i_executor::structures::{CommitBatchInfo, StoredBatchInfo, SUPPORTED_ENCODING_VERSION},
    Tokenizable, Tokenize,
};
use crate::i_executor::structures::CommitBoojumOSBatchInfo;

/// Input required to encode `commitBatches` call for a contract
#[derive(Debug)]
pub struct CommitBatches<'a> {
    pub last_committed_l1_batch: &'a L1BatchWithMetadata,
    pub l1_batches: &'a [L1BatchWithMetadata],
    pub pubdata_da: PubdataSendingMode,
    pub mode: L1BatchCommitmentMode,
}

impl Tokenize for &CommitBatches<'_> {
    fn into_tokens(self) -> Vec<Token> {
        assert_eq!(self.l1_batches.len(), 1, "Only one batch can be committed at a time in zk os");
        let l1_batch = self.l1_batches.first().unwrap();

        // note: for now we convert `CommitBoojumOSBatchInfo` to `StoredBatchInfo`
        // as most fields are the same
        // this will change when we refactor `L1BatchWithMetadata` - we'll use this new base struct instead
        let stored_batch_info = StoredBatchInfo::new(
            CommitBoojumOSBatchInfo::new(
                self.mode,
                self.last_committed_l1_batch,
                self.pubdata_da,
            )
        );
        let commit_boojum_os_batch_info =
            CommitBoojumOSBatchInfo::new(
                self.mode,
                l1_batch,
                self.pubdata_da,
            );
        tracing::info!(
            "Encoding CommitBatches for batch {}. {:?}; {:?}; Stored Batch Info hash: {}",
            self.l1_batches[0].header.number.0,
            stored_batch_info,
            commit_boojum_os_batch_info,
            stored_batch_info.hash()
        );

        let mut encoded_data = encode(&[
            stored_batch_info.into_token(),
            Token::Array(vec![Token::Tuple(commit_boojum_os_batch_info.into_tokens())]),
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
