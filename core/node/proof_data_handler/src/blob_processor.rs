use std::fmt;

use zksync_types::{basic_fri_types::Eip4844Blobs, L1BatchNumber};

#[derive(Debug)]
pub struct ValidiumBlobProcessor;

#[derive(Debug)]
pub struct RollupBlobProcessor;

pub trait BlobProcessor: 'static + fmt::Debug + Send + Sync {
    fn process_blobs(&self, l1_batch_number: L1BatchNumber, blobs: Option<Vec<u8>>)
        -> Eip4844Blobs;
}

impl BlobProcessor for ValidiumBlobProcessor {
    fn process_blobs(&self, _: L1BatchNumber, _: Option<Vec<u8>>) -> Eip4844Blobs {
        Eip4844Blobs::empty()
    }
}

impl BlobProcessor for RollupBlobProcessor {
    fn process_blobs(
        &self,
        l1_batch_number: L1BatchNumber,
        blobs: Option<Vec<u8>>,
    ) -> Eip4844Blobs {
        let blobs = &blobs.unwrap_or_else(|| {
            panic!("expected pubdata, but it is not available for batch {l1_batch_number:?}")
        });
        Eip4844Blobs::decode(blobs).expect("failed to decode EIP-4844 blobs")
    }
}
