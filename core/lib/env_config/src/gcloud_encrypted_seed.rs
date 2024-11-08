use google_cloud_kms::{
    client::{Client as kms_client, ClientConfig as kms_config},
    grpc::kms::v1::DecryptRequest,
};
use google_cloud_storage::{
    client::{Client as storage_client, ClientConfig as storage_config},
    http::objects::{download::Range, get::GetObjectRequest},
};
use hex;
use tokio;

pub fn retrieve_seed_from_gcloud(decrypt_key: String, bucket_name: String)-> String {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Download encryped seed for google cloud storage
    let encrypted_seed_from_gcs = rt.block_on(download_seed_from_gcs(bucket_name));
    // Decrypt seed with kms key
    let decrypted_seed = rt.block_on(decrypt_seed_with_kms(&encrypted_seed_from_gcs, decrypt_key));

    decrypted_seed
}

async fn decrypt_seed_with_kms(encrypted_seed: &[u8], decrypt_key: String) -> String {
    let config = kms_config::default().with_auth().await.unwrap();
    let client = kms_client::new(config).await.unwrap();

    let request = DecryptRequest {
        name: decrypt_key,
        ciphertext: encrypted_seed.to_vec(),
        additional_authenticated_data: vec![],
        ciphertext_crc32c: None,
        additional_authenticated_data_crc32c: None,
    };
    let decrypted_seed = client
        .decrypt(request, None)
        .await
        .expect("Failed to decrypt seed");
    hex::encode(decrypted_seed.plaintext)
}
// Downloads the seed from the specified GCS bucket.
async fn download_seed_from_gcs(bucket_name: String) -> Vec<u8> {
    let config = storage_config::default().with_auth().await.unwrap();
    let client = storage_client::new(config);

    // Download the file
    let encrypted_seed = client
        .download_object(
            &GetObjectRequest {
                bucket: bucket_name,
                object: "seed.bin".to_string(),
                ..Default::default()
            },
            &Range::default(),
        )
        .await
        .expect("Failed to download seed");
    encrypted_seed
}