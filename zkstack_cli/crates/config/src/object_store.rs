//! Mirror of object store configuration. Only includes fields / enum variants actually used by `zkstack` scripting.

#[derive(Debug)]
pub enum ObjectStoreMode {
    GCSWithCredentialFile {
        bucket_base_url: String,
        gcs_credential_file_path: String,
    },
    FileBacked {
        file_backed_base_path: String,
    },
}

#[derive(Debug)]
pub struct ObjectStoreConfig {
    pub mode: ObjectStoreMode,
    pub max_retries: u16,
}
