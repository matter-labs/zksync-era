use std::io::{Read, Write};

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use zksync_object_store::{Bucket, StoredObject, _reexports::BoxedError};
use zksync_types::L1BatchNumber;

/// Used as a wrapper for the pubdata to be stored in the GCS.
#[derive(Debug)]
pub struct StorablePubdata {
    pub data: Vec<u8>,
}

impl StoredObject for StorablePubdata {
    const BUCKET: Bucket = Bucket::DataAvailability;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_{key}_pubdata.gzip")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&self.data[..])?;
        encoder.finish().map_err(From::from)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut decompressed_bytes = Vec::new();
        decoder
            .read_to_end(&mut decompressed_bytes)
            .map_err(BoxedError::from)?;

        Ok(Self {
            data: decompressed_bytes,
        })
    }
}
