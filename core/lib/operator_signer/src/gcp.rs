use anyhow::Context;
use base64::Engine;
use google_cloud_auth::{
    project::{self, Config},
    token::Token,
    token_source::TokenSource,
};
use reqwest::Client;
use secp256k1::{ecdsa::RecoveryId, Message as SecpMessage, PublicKey, SECP256K1};
use serde::Deserialize;
use zksync_basic_types::{web3, Address, H256};

const KMS_SCOPES: &[&str] = &["https://www.googleapis.com/auth/cloudkms"];

/// A GCP KMS signer that implements ECDSA signing via the Cloud KMS REST API.
///
/// The signer caches the public key and Ethereum address after first retrieval.
#[derive(Clone)]
pub(crate) struct GcpKmsSigner {
    resource_name: String,
    public_key: PublicKey,
    address: Address,
    http_client: Client,
    token_source: std::sync::Arc<dyn TokenSource>,
}

impl std::fmt::Debug for GcpKmsSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcpKmsSigner")
            .field("resource_name", &self.resource_name)
            .field("address", &self.address)
            .finish()
    }
}

/// Response from the KMS getPublicKey endpoint.
#[derive(Deserialize)]
struct GetPublicKeyResponse {
    pem: String,
}

/// Response from the KMS asymmetricSign endpoint.
#[derive(Deserialize)]
struct AsymmetricSignResponse {
    #[serde(with = "base64_serde")]
    signature: Vec<u8>,
}

mod base64_serde {
    use base64::Engine;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

impl GcpKmsSigner {
    /// Creates a new GCP KMS signer by fetching the public key from KMS.
    ///
    /// The key must use the `EC_SIGN_SECP256K1_SHA256` algorithm.
    pub(crate) async fn new(resource_name: String) -> anyhow::Result<Self> {
        let config = Config {
            scopes: Some(KMS_SCOPES),
            ..Default::default()
        };

        #[allow(deprecated)]
        let token_source: Box<dyn TokenSource> = project::create_token_source(config)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create GCP token source: {e}"))?;

        let token_source: std::sync::Arc<dyn TokenSource> = token_source.into();
        let http_client = Client::new();

        // Fetch the public key from KMS.
        let public_key =
            fetch_public_key(&http_client, &token_source, &resource_name).await?;

        // Derive Ethereum address from public key.
        let uncompressed = public_key.serialize_uncompressed();
        let hash = zksync_basic_types::web3::keccak256(&uncompressed[1..]);
        let mut address = Address::zero();
        address.as_bytes_mut().copy_from_slice(&hash[12..]);

        Ok(Self {
            resource_name,
            public_key,
            address,
            http_client,
            token_source,
        })
    }

    pub(crate) fn address(&self) -> Address {
        self.address
    }

    /// Signs a 32-byte hash using GCP KMS and returns a web3::Signature.
    ///
    /// The `chain_id` is used to compute the `v` value for legacy transactions.
    /// Pass `Some(chain_id)` for legacy transactions, `None` for EIP-2930+ transactions.
    pub(crate) async fn sign_hash(
        &self,
        hash: &H256,
        chain_id: Option<u64>,
    ) -> anyhow::Result<web3::Signature> {
        let token = self
            .token_source
            .token()
            .await
            .map_err(|e| anyhow::anyhow!("failed to get GCP access token: {e}"))?;

        let der_signature =
            kms_asymmetric_sign(&self.http_client, &token, &self.resource_name, hash).await?;

        // Parse the DER-encoded ECDSA signature.
        let secp_sig = secp256k1::ecdsa::Signature::from_der(&der_signature)
            .context("failed to parse DER signature from KMS")?;
        let compact = secp_sig.serialize_compact();

        // Determine recovery ID by trying both 0 and 1.
        let message =
            SecpMessage::from_slice(hash.as_bytes()).context("failed to create secp256k1 message")?;
        let mut recovery_id: Option<i32> = None;
        for rid in 0..=1i32 {
            let rec_id = RecoveryId::from_i32(rid).expect("valid recovery id");
            let recoverable = secp256k1::ecdsa::RecoverableSignature::from_compact(&compact, rec_id)
                .context("failed to create recoverable signature")?;
            if let Ok(recovered) = SECP256K1.recover_ecdsa(&message, &recoverable) {
                if recovered == self.public_key {
                    recovery_id = Some(rid);
                    break;
                }
            }
        }

        let rid = recovery_id.context("failed to determine recovery ID for KMS signature")?;

        let v = if let Some(chain_id) = chain_id {
            rid as u64 + 35 + chain_id * 2
        } else {
            rid as u64
        };

        let r = H256::from_slice(&compact[..32]);
        let s = H256::from_slice(&compact[32..]);

        Ok(web3::Signature { r, s, v })
    }

    /// Signs a 32-byte hash and returns a PackedEthSignature (recovery_id without chain_id encoding).
    pub(crate) async fn sign_hash_raw(
        &self,
        hash: &H256,
    ) -> anyhow::Result<zksync_crypto_primitives::PackedEthSignature> {
        let token = self
            .token_source
            .token()
            .await
            .map_err(|e| anyhow::anyhow!("failed to get GCP access token: {e}"))?;

        let der_signature =
            kms_asymmetric_sign(&self.http_client, &token, &self.resource_name, hash).await?;

        let secp_sig = secp256k1::ecdsa::Signature::from_der(&der_signature)
            .context("failed to parse DER signature from KMS")?;
        let compact = secp_sig.serialize_compact();

        let message =
            SecpMessage::from_slice(hash.as_bytes()).context("failed to create secp256k1 message")?;
        let mut recovery_id: Option<i32> = None;
        for rid in 0..=1i32 {
            let rec_id = RecoveryId::from_i32(rid).expect("valid recovery id");
            let recoverable = secp256k1::ecdsa::RecoverableSignature::from_compact(&compact, rec_id)
                .context("failed to create recoverable signature")?;
            if let Ok(recovered) = SECP256K1.recover_ecdsa(&message, &recoverable) {
                if recovered == self.public_key {
                    recovery_id = Some(rid);
                    break;
                }
            }
        }

        let rid = recovery_id.context("failed to determine recovery ID for KMS signature")?;
        let r = H256::from_slice(&compact[..32]);
        let s = H256::from_slice(&compact[32..]);

        Ok(zksync_crypto_primitives::PackedEthSignature::from_rsv(
            &r,
            &s,
            rid as u8,
        ))
    }
}

/// Fetches the public key from GCP KMS.
async fn fetch_public_key(
    client: &Client,
    token_source: &std::sync::Arc<dyn TokenSource>,
    resource_name: &str,
) -> anyhow::Result<PublicKey> {
    let token = token_source
        .token()
        .await
        .map_err(|e| anyhow::anyhow!("failed to get GCP access token: {e}"))?;

    let url = format!(
        "https://cloudkms.googleapis.com/v1/{resource_name}/publicKey"
    );

    let resp = client
        .get(&url)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .context("KMS getPublicKey request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "KMS getPublicKey failed with status {status}: {body}. \
             Ensure the key '{resource_name}' exists and uses EC_SIGN_SECP256K1_SHA256 algorithm"
        );
    }

    let response: GetPublicKeyResponse = resp
        .json()
        .await
        .context("failed to parse KMS getPublicKey response")?;

    parse_pem_public_key(&response.pem)
}

/// Signs a digest using GCP KMS asymmetricSign endpoint.
async fn kms_asymmetric_sign(
    client: &Client,
    token: &Token,
    resource_name: &str,
    digest: &H256,
) -> anyhow::Result<Vec<u8>> {
    let url = format!(
        "https://cloudkms.googleapis.com/v1/{resource_name}:asymmetricSign"
    );

    // Send the hash as a SHA-256 digest. KMS signs the provided digest directly
    // without re-hashing (this is standard practice for Ethereum signing with KMS).
    let digest_b64 = base64::engine::general_purpose::STANDARD.encode(digest.as_bytes());
    let body = serde_json::json!({
        "digest": {
            "sha256": digest_b64
        }
    });

    let resp = client
        .post(&url)
        .bearer_auth(&token.access_token)
        .json(&body)
        .send()
        .await
        .context("KMS asymmetricSign request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("KMS asymmetricSign failed with status {status}: {body}");
    }

    let response: AsymmetricSignResponse = resp
        .json()
        .await
        .context("failed to parse KMS asymmetricSign response")?;

    Ok(response.signature)
}

/// Parses a PEM-encoded public key (SubjectPublicKeyInfo) and extracts the secp256k1 public key.
fn parse_pem_public_key(pem: &str) -> anyhow::Result<PublicKey> {
    // Extract base64 content from PEM.
    let b64: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();

    let der = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .context("failed to decode PEM base64")?;

    // The SubjectPublicKeyInfo DER structure for secp256k1 contains:
    // SEQUENCE {
    //   SEQUENCE { OID(ecPublicKey), OID(secp256k1) }
    //   BIT STRING { 0x00, <public_key_bytes> }
    // }
    //
    // We search for the BIT STRING (tag 0x03) and extract the public key after the
    // padding byte (0x00).
    let key_bytes = extract_bit_string_content(&der)
        .context("failed to extract public key from DER SubjectPublicKeyInfo")?;

    PublicKey::from_slice(key_bytes)
        .context("failed to parse secp256k1 public key from DER content")
}

/// Extracts the content of the BIT STRING from a DER-encoded SubjectPublicKeyInfo.
/// Returns the public key bytes (after the padding byte).
fn extract_bit_string_content(der: &[u8]) -> Option<&[u8]> {
    // Walk through the DER to find the BIT STRING (tag 0x03).
    let mut i = 0;
    while i < der.len() {
        let tag = der[i];
        i += 1;
        if i >= der.len() {
            return None;
        }

        // Parse length.
        let (length, consumed) = parse_der_length(&der[i..])?;
        i += consumed;

        if tag == 0x03 {
            // BIT STRING found. First byte is the number of unused bits (should be 0x00).
            if i >= der.len() || der[i] != 0x00 {
                return None;
            }
            // Return the content after the padding byte.
            let start = i + 1;
            let end = i + length;
            if end > der.len() {
                return None;
            }
            return Some(&der[start..end]);
        }

        // For SEQUENCE (0x30), descend into it.
        if tag == 0x30 {
            // Don't skip; process children.
            continue;
        }

        // For other tags, skip the content.
        i += length;
    }
    None
}

/// Parses a DER length field and returns (length, bytes_consumed).
fn parse_der_length(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() {
        return None;
    }
    if data[0] < 0x80 {
        Some((data[0] as usize, 1))
    } else {
        let num_bytes = (data[0] & 0x7f) as usize;
        if num_bytes == 0 || data.len() < 1 + num_bytes {
            return None;
        }
        let mut length: usize = 0;
        for &b in &data[1..1 + num_bytes] {
            length = length.checked_shl(8)?.checked_add(b as usize)?;
        }
        Some((length, 1 + num_bytes))
    }
}

/// Validates the format of a KMS resource name.
///
/// Expected format:
/// `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`
#[allow(dead_code)]
pub(crate) fn validate_kms_resource_name(resource_name: &str) -> anyhow::Result<()> {
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

    let _version: u64 = parts[9]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid key version number: '{}'", parts[9]))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_kms_resource_name_valid() {
        let resource = "projects/my-project/locations/us-central1/keyRings/my-ring/cryptoKeys/my-key/cryptoKeyVersions/1";
        validate_kms_resource_name(resource).unwrap();
    }

    #[test]
    fn test_validate_kms_resource_name_invalid() {
        assert!(validate_kms_resource_name("invalid/resource/name").is_err());
        assert!(validate_kms_resource_name("").is_err());
        assert!(
            validate_kms_resource_name(
                "projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/notanumber"
            )
            .is_err()
        );
    }

    #[test]
    fn test_parse_der_length() {
        // Short form: length < 128
        assert_eq!(parse_der_length(&[0x42]), Some((0x42, 1)));
        // Long form: 1 byte
        assert_eq!(parse_der_length(&[0x81, 0x80]), Some((128, 2)));
        // Long form: 2 bytes
        assert_eq!(parse_der_length(&[0x82, 0x01, 0x00]), Some((256, 3)));
        // Empty
        assert_eq!(parse_der_length(&[]), None);
    }
}
