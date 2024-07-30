use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

pub(crate) enum ProcessorError {
    ObjectStore(ObjectStoreError),
    Dal(DalError),
    Serialization(bincode::Error),
    InvalidProof,
}

impl From<ObjectStoreError> for ProcessorError {
    fn from(err: ObjectStoreError) -> Self {
        Self::ObjectStore(err)
    }
}

impl From<DalError> for ProcessorError {
    fn from(err: DalError) -> Self {
        Self::Dal(err)
    }
}

impl From<bincode::Error> for ProcessorError {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err)
    }
}
