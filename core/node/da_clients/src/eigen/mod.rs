mod blob_info;
mod client;
mod errors;
mod sdk;
mod verifier;

pub use self::client::{EigenClient, GetBlobData};
#[allow(clippy::all)]
pub(crate) mod disperser {
    include!("generated/disperser.rs");
}

#[allow(clippy::all)]
pub(crate) mod common {
    include!("generated/common.rs");
}

#[cfg(test)]
pub fn test_eigenda_config() -> zksync_config::EigenConfig {
    use std::str::FromStr;

    zksync_config::EigenConfig {
                disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
                settlement_layer_confirmation_depth: 0,
                eigenda_eth_rpc: Some(zksync_basic_types::url::SensitiveUrl::from_str("https://ethereum-holesky-rpc.publicnode.com").unwrap()), // Safe to unwrap, never fails
                eigenda_svc_manager_address: zksync_basic_types::H160([
                    0xd4, 0xa7, 0xe1, 0xbd, 0x80, 0x15, 0x05, 0x72, 0x93, 0xf0, 0xd0, 0xa5, 0x57, 0x08, 0x8c,
                    0x28, 0x69, 0x42, 0xe8, 0x4b,
                ]), // DEFAULT_EIGENDA_SVC_MANAGER_ADDRESS
                wait_for_finalization: false,
                authenticated: false,
                points_source: zksync_config::configs::da_client::eigen::PointsSource::Url((
                    "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
                    "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
                ))
        }
}
