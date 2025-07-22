use std::path::PathBuf;

use smart_config::{DescribeConfig, DeserializeConfig};

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ObjectStoreConfig {
    #[config(flatten)]
    pub mode: ObjectStoreMode,
    /// Max retries when working with the object store. Only transient errors (e.g., network ones) are retried.
    #[config(default_t = 5)]
    pub max_retries: u16,
    /// Path to local directory that will be used to mirror store objects locally. If not specified, no mirroring will be used.
    /// The directory layout is identical to [`ObjectStoreMode::FileBacked`].
    ///
    /// Mirroring is primarily useful for local development and testing; it might not provide substantial performance benefits
    /// if the Internet connection used by the app is fast enough.
    ///
    /// **Important.** Mirroring logic assumes that objects in the underlying store are immutable. If this is not the case,
    /// the mirrored objects may become stale.
    pub local_mirror_path: Option<PathBuf>,
}

impl ObjectStoreConfig {
    pub fn for_tests() -> Self {
        Self {
            mode: ObjectStoreMode::FileBacked {
                file_backed_base_path: "./artifacts".into(),
            },
            max_retries: 5,
            local_mirror_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "mode")]
pub enum ObjectStoreMode {
    /// Writable GCS bucket with ambient authentication.
    #[config(alias = "Gcs")]
    GCS {
        /// Name or URL of the bucket.
        bucket_base_url: String,
    },
    /// Publicly available GCS bucket.
    #[config(alias = "GcsAnonymousReadOnly")]
    GCSAnonymousReadOnly {
        /// Name or URL of the bucket.
        bucket_base_url: String,
    },
    /// GCS bucket with credential file authentication.
    #[config(alias = "GcsWithCredentialFile")]
    GCSWithCredentialFile {
        /// Name or URL of the bucket.
        bucket_base_url: String,
        /// Path to the credentials file.
        gcs_credential_file_path: PathBuf,
    },
    /// Publicly available S3-compatible bucket.
    S3AnonymousReadOnly {
        /// Name or URL of the bucket.
        bucket_base_url: String,
        /// Allows overriding AWS S3 API endpoint, e.g. to use another S3-compatible store provider.
        endpoint: Option<String>,
        /// Allows specifying bucket region (inferred from the env by default).
        region: Option<String>,
    },
    /// S3-compatible bucket with credential file authentication.
    S3WithCredentialFile {
        /// Name or URL of the bucket.
        bucket_base_url: String,
        /// Path to the credentials file.
        s3_credential_file_path: PathBuf,
        /// Allows overriding AWS S3 API endpoint, e.g. to use another S3-compatible store provider.
        endpoint: Option<String>,
        /// Allows specifying bucket region (inferred from the env by default).
        region: Option<String>,
    },
    /// Stores files in a local filesystem. Mostly useful for local testing.
    #[config(default)]
    FileBacked {
        /// Path to the root directory for storage.
        file_backed_base_path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test, test_complete, Tester},
        Environment, Yaml,
    };

    use super::*;

    fn expected_gcs_config(bucket_base_url: &str) -> ObjectStoreConfig {
        ObjectStoreConfig {
            mode: ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url: bucket_base_url.to_owned(),
                gcs_credential_file_path: "/path/to/credentials.json".into(),
            },
            max_retries: 5,
            local_mirror_path: Some("/var/cache".into()),
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            OBJECT_STORE_BUCKET_BASE_URL="/base/url"
            OBJECT_STORE_MODE="GCSWithCredentialFile"
            OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials.json"
            OBJECT_STORE_MAX_RETRIES="5"
            OBJECT_STORE_LOCAL_MIRROR_PATH="/var/cache"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("OBJECT_STORE_");

        let config: ObjectStoreConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_gcs_config("/base/url"));
    }

    #[test]
    fn file_backed_from_env() {
        let env = r#"
            OBJECT_STORE_MODE="FileBacked"
            OBJECT_STORE_FILE_BACKED_BASE_PATH="artifacts"
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("OBJECT_STORE_");

        let config: ObjectStoreConfig = test(env).unwrap();
        assert_eq!(
            config.mode,
            ObjectStoreMode::FileBacked {
                file_backed_base_path: "artifacts".into(),
            }
        );
    }

    #[test]
    fn public_bucket_from_env() {
        let env = r#"
            PUBLIC_OBJECT_STORE_BUCKET_BASE_URL="/public_base_url"
            PUBLIC_OBJECT_STORE_MODE="GCSAnonymousReadOnly"
            PUBLIC_OBJECT_STORE_MAX_RETRIES="3"
            PUBLIC_OBJECT_STORE_LOCAL_MIRROR_PATH=/var/cache
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("PUBLIC_OBJECT_STORE_");

        let config: ObjectStoreConfig = test_complete(env).unwrap();
        assert_eq!(config.max_retries, 3);
        assert_eq!(
            config.mode,
            ObjectStoreMode::GCSAnonymousReadOnly {
                bucket_base_url: "/public_base_url".to_owned(),
            }
        );
    }

    #[test]
    fn file_backed_from_yaml() {
        let yaml = r#"
          # Read by old system
          file_backed:
            file_backed_base_path: ./chains/era/artifacts/

          mode: FileBacked
          file_backed_base_path: ./chains/era/artifacts/
          max_retries: 10
          local_mirror_path: /var/cache
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ObjectStoreConfig = test_complete(yaml).unwrap();
        assert_eq!(
            config.mode,
            ObjectStoreMode::FileBacked {
                file_backed_base_path: "./chains/era/artifacts/".into(),
            }
        );
    }

    #[test]
    fn public_bucket_from_yaml_with_enum_coercion() {
        let yaml = r#"
          gcs_anonymous_read_only:
            bucket_base_url: /public_base_url
          max_retries: 3
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ObjectStoreConfig = Tester::default().coerce_serde_enums().test(yaml).unwrap();
        assert_eq!(config.max_retries, 3);
        assert_eq!(
            config.mode,
            ObjectStoreMode::GCSAnonymousReadOnly {
                bucket_base_url: "/public_base_url".to_owned(),
            }
        );
    }
}
