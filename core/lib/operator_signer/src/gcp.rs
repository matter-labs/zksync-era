use alloy_signer_gcp::{GcpKeyRingRef, GcpSigner, KeySpecifier};
use anyhow::Context;
use gcloud_sdk::{
    google::cloud::kms::v1::key_management_service_client::KeyManagementServiceClient, GoogleApi,
};

/// Creates a GCP KMS signer from a resource name.
///
/// The key must use the `EC_SIGN_SECP256K1_SHA256` algorithm.
/// If the key uses a different algorithm (e.g. RSA, P-256), signer creation
/// will fail because the public key cannot be decoded as a secp256k1 key.
pub(crate) async fn create_gcp_signer(resource_name: &str) -> anyhow::Result<GcpSigner> {
    let parts = parse_kms_resource_name(resource_name)?;
    let keyring = GcpKeyRingRef::new(&parts.project_id, &parts.location, &parts.keyring_name);
    let specifier = KeySpecifier::new(keyring, &parts.key_id, parts.version);

    let client = GoogleApi::from_function(
        KeyManagementServiceClient::new,
        "https://cloudkms.googleapis.com",
        None,
    )
    .await
    .context("failed to create GCP KMS client")?;

    GcpSigner::new(client, specifier, None).await.map_err(|e| {
        anyhow::anyhow!(
            "failed to initialize GCP KMS signer for '{resource_name}': {e}. \
                 Ensure the key uses EC_SIGN_SECP256K1_SHA256 algorithm"
        )
    })
}

/// Parsed components of a KMS resource name.
struct KmsResourceParts {
    project_id: String,
    location: String,
    keyring_name: String,
    key_id: String,
    version: u64,
}

/// Parses a KMS resource name into its components.
///
/// Expected format:
/// `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`
fn parse_kms_resource_name(resource_name: &str) -> anyhow::Result<KmsResourceParts> {
    let parts: Vec<&str> = resource_name.split('/').collect();
    anyhow::ensure!(
        parts.len() == 10
            && parts[0] == "projects"
            && parts[2] == "locations"
            && parts[4] == "keyRings"
            && parts[6] == "cryptoKeys"
            && parts[8] == "cryptoKeyVersions",
        "invalid KMS resource name format: expected \
         'projects/{{project}}/locations/{{location}}/keyRings/{{ring}}/cryptoKeys/{{key}}/cryptoKeyVersions/{{version}}', \
         got '{resource_name}'"
    );

    let version: u64 = parts[9]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid key version number: '{}'", parts[9]))?;

    Ok(KmsResourceParts {
        project_id: parts[1].to_owned(),
        location: parts[3].to_owned(),
        keyring_name: parts[5].to_owned(),
        key_id: parts[7].to_owned(),
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kms_resource_name_valid() {
        let resource = "projects/my-project/locations/us-central1/keyRings/my-ring/cryptoKeys/my-key/cryptoKeyVersions/1";
        let parts = parse_kms_resource_name(resource).unwrap();
        assert_eq!(parts.project_id, "my-project");
        assert_eq!(parts.location, "us-central1");
        assert_eq!(parts.keyring_name, "my-ring");
        assert_eq!(parts.key_id, "my-key");
        assert_eq!(parts.version, 1);
    }

    #[test]
    fn test_parse_kms_resource_name_invalid() {
        assert!(parse_kms_resource_name("invalid/resource/name").is_err());
        assert!(parse_kms_resource_name("").is_err());
        assert!(parse_kms_resource_name(
            "projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/notanumber"
        )
        .is_err());
    }
}
