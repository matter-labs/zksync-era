use alloy_signer_gcp::{GcpKeyRingRef, GcpSigner, KeySpecifier};
use gcloud_sdk::{
    google::cloud::kms::v1::key_management_service_client::KeyManagementServiceClient, GoogleApi,
};

/// Creates a [`GcpSigner`] from a full KMS resource name.
///
/// Resource name format:
/// `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`
pub(crate) async fn create_signer(resource_name: &str) -> anyhow::Result<GcpSigner> {
    let (keyring, key_name, key_version) = parse_resource_name(resource_name)?;

    let client = GoogleApi::from_function(
        KeyManagementServiceClient::new,
        "https://cloudkms.googleapis.com",
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to create GCP KMS client: {e}"))?;

    let specifier = KeySpecifier::new(keyring, &key_name, key_version);
    GcpSigner::new(client, specifier, None)
        .await
        .map_err(|e| anyhow::anyhow!("failed to initialize GCP KMS signer: {e}"))
}

/// Parses a full KMS resource name into its components.
fn parse_resource_name(resource_name: &str) -> anyhow::Result<(GcpKeyRingRef, String, u64)> {
    let parts: Vec<&str> = resource_name.split('/').collect();
    anyhow::ensure!(
        parts.len() == 10
            && parts[0] == "projects"
            && parts[2] == "locations"
            && parts[4] == "keyRings"
            && parts[6] == "cryptoKeys"
            && parts[8] == "cryptoKeyVersions",
        "invalid KMS resource name: {resource_name}"
    );

    let key_version: u64 = parts[9]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid key version number: '{}'", parts[9]))?;

    let keyring = GcpKeyRingRef::new(parts[1], parts[3], parts[5]);
    Ok((keyring, parts[7].to_string(), key_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resource_name_valid() {
        let resource = "projects/my-project/locations/us-central1/keyRings/my-ring/cryptoKeys/my-key/cryptoKeyVersions/1";
        let (keyring, key_name, version) = parse_resource_name(resource).unwrap();
        assert_eq!(keyring.google_project_id, "my-project");
        assert_eq!(keyring.location, "us-central1");
        assert_eq!(keyring.name, "my-ring");
        assert_eq!(key_name, "my-key");
        assert_eq!(version, 1);
    }

    #[test]
    fn test_parse_resource_name_invalid() {
        assert!(parse_resource_name("invalid/resource/name").is_err());
        assert!(parse_resource_name("").is_err());
        assert!(parse_resource_name(
            "projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/notanumber"
        )
        .is_err());
    }
}
