use std::path::PathBuf;

use serde::Deserialize;
use smart_config::{DescribeConfig, DeserializeConfig};

// TODO: remove `#[derive(Deserialize)]` once env-based config in EN is reworked

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
pub struct ObjectStoreConfig {
    #[config(flatten)]
    #[serde(flatten)]
    pub mode: ObjectStoreMode,
    #[config(default_t = 5)]
    #[serde(default = "ObjectStoreConfig::default_max_retries")]
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
    const fn default_max_retries() -> u16 {
        5
    }

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

#[derive(Debug, Clone, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
#[config(tag = "mode")]
#[serde(tag = "mode")]
pub enum ObjectStoreMode {
    #[config(alias = "Gcs")]
    GCS { bucket_base_url: String },
    #[config(alias = "GcsAnonymousReadOnly")]
    GCSAnonymousReadOnly { bucket_base_url: String },
    #[config(alias = "GcsWithCredentialFile")]
    GCSWithCredentialFile {
        bucket_base_url: String,
        gcs_credential_file_path: String,
    },
    S3AnonymousReadOnly {
        bucket_base_url: String,
        endpoint: Option<String>,
        region: Option<String>,
    },
    S3WithCredentialFile {
        bucket_base_url: String,
        s3_credential_file_path: String,
        endpoint: Option<String>,
        region: Option<String>,
    },
    #[config(default)]
    FileBacked { file_backed_base_path: PathBuf },
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
                gcs_credential_file_path: "/path/to/credentials.json".to_owned(),
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
