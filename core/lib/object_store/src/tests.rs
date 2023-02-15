use crate::object_store::{create_object_store, ObjectStoreMode};
use expanduser::expanduser;
use std::env;

#[test]
fn test_object_store_in_memory_creation() {
    let object_store = create_object_store(ObjectStoreMode::FileBacked, "artifacts".to_string());
    assert_eq!("FileBackedStore", object_store.get_store_type());
}

#[test]
fn test_object_store_gcs_creation() {
    set_object_store_environment_variable();
    let object_store = create_object_store(ObjectStoreMode::GCS, "".to_string());
    assert_eq!("GoogleCloudStorage", object_store.get_store_type());
}

fn set_object_store_environment_variable() {
    let path = expanduser("~/gcloud/service_account.json").unwrap();
    env::set_var("OBJECT_STORE_SERVICE_ACCOUNT_PATH", path);
    env::set_var("OBJECT_STORE_BUCKET_BASE_URL", "/base/url");
    env::set_var("OBJECT_STORE_MODE", "GCS");
    env::set_var("OBJECT_STORE_FILE_BACKED_BASE_PATH", "/base/url");
}
