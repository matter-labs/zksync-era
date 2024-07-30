use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

pub(crate) enum ProcessorError {
    ObjectStore(ObjectStoreError),
    Dal(DalError),
}
