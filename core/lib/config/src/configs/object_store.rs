use std::path::PathBuf;

use smart_config::{DescribeConfig, DeserializeConfig};

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ObjectStoreConfig {
    #[config(flatten)]
    pub mode: ObjectStoreMode,
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

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "mode", derive(Default))]
pub enum ObjectStoreMode {
    GCS {
        bucket_base_url: String,
    },
    GCSAnonymousReadOnly {
        bucket_base_url: String,
    },
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
    FileBacked {
        #[config(default_t = "./artifacts".into())]
        file_backed_base_path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test, Environment, Yaml};

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

        let config: ObjectStoreConfig = test(env).unwrap();
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
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("PUBLIC_OBJECT_STORE_");

        let config: ObjectStoreConfig = test(env).unwrap();
        assert_eq!(config.max_retries, 3);
        assert_eq!(
            config.mode,
            ObjectStoreMode::GCSAnonymousReadOnly {
                bucket_base_url: "/public_base_url".to_owned(),
            }
        );
    }

    // Migration path: use tagged enums for object stores
    #[test]
    fn file_backed_from_yaml() {
        let yaml = r#"
          # Read by old system
          file_backed:
            file_backed_base_path: ./chains/era/artifacts/

          mode: FileBacked
          file_backed_base_path: ./chains/era/artifacts/
          max_retries: 10
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ObjectStoreConfig = test(yaml).unwrap();
        assert_eq!(
            config.mode,
            ObjectStoreMode::FileBacked {
                file_backed_base_path: "./chains/era/artifacts/".into(),
            }
        );
    }
}
