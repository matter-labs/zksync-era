use std::str::FromStr;

use anyhow::Context;
use zksync_config::{
    configs::{
        self,
        da_client::{
            avail::{AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig},
            celestia::CelestiaConfig,
            eigenda::{V1Config, V2Config, VersionSpecificConfig},
            DAClientConfig::{Avail, Celestia, EigenDA, NoDA, ObjectStore},
        },
    },
    EigenDAConfig,
};
use zksync_protobuf::{required, ProtoRepr};
use zksync_types::url::SensitiveUrl;

use crate::{
    parse_h160,
    proto::{
        da_client::{self as proto},
        object_store as object_store_proto,
    },
};

impl ProtoRepr for proto::DataAvailabilityClient {
    type Type = configs::DAClientConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let config = required(&self.config).context("da_client config")?;
        let client = match config {
            proto::data_availability_client::Config::Avail(conf) => Avail(AvailConfig {
                bridge_api_url: required(&conf.bridge_api_url)
                    .context("bridge_api_url")?
                    .clone(),
                timeout_ms: *required(&conf.timeout_ms).context("timeout_ms")? as usize,
                config: match conf.config.as_ref() {
                    Some(proto::avail_config::Config::FullClient(full_client_conf)) => {
                        AvailClientConfig::FullClient(AvailDefaultConfig {
                            api_node_url: required(&full_client_conf.api_node_url)
                                .context("api_node_url")?
                                .clone(),
                            app_id: *required(&full_client_conf.app_id).context("app_id")?,
                            finality_state: full_client_conf.finality_state.clone(),
                            dispatch_timeout_ms: full_client_conf.dispatch_timeout_ms,
                        })
                    }
                    Some(proto::avail_config::Config::GasRelay(gas_relay_conf)) => {
                        AvailClientConfig::GasRelay(AvailGasRelayConfig {
                            gas_relay_api_url: required(&gas_relay_conf.gas_relay_api_url)
                                .context("gas_relay_api_url")?
                                .clone(),
                            max_retries: *required(&gas_relay_conf.max_retries)
                                .context("max_retries")?
                                as usize,
                        })
                    }
                    None => return Err(anyhow::anyhow!("Invalid Avail DA configuration")),
                },
            }),
            proto::data_availability_client::Config::Celestia(conf) => Celestia(CelestiaConfig {
                api_node_url: required(&conf.api_node_url).context("namespace")?.clone(),
                namespace: required(&conf.namespace).context("namespace")?.clone(),
                chain_id: required(&conf.chain_id).context("chain_id")?.clone(),
                timeout_ms: *required(&conf.timeout_ms).context("timeout_ms")?,
            }),
            proto::data_availability_client::Config::Eigenda(conf) => EigenDA(EigenDAConfig {
                disperser_rpc: required(&conf.disperser_rpc)
                    .context("disperser_rpc")?
                    .clone(),
                eigenda_eth_rpc: conf
                    .eigenda_eth_rpc
                    .clone()
                    .map(|x| SensitiveUrl::from_str(&x).context("eigenda_eth_rpc"))
                    .transpose()
                    .context("eigenda_eth_rpc")?,
                authenticated: *required(&conf.authenticated).context("authenticated")?,
                version_specific: match conf.client_version.clone() {
                    Some(proto::eigen_da_config::ClientVersion::V1(v1_conf)) => {
                        VersionSpecificConfig::V1(V1Config {
                            custom_quorum_numbers: v1_conf
                                .custom_quorum_numbers
                                .iter()
                                .map(|x| u8::try_from(*x).context("custom_quorum_numbers"))
                                .collect::<anyhow::Result<Vec<u8>>>()?,
                            eigenda_svc_manager_address: required(
                                &v1_conf.eigenda_svc_manager_address,
                            )
                            .and_then(|x| parse_h160(x))
                            .context("eigenda_svc_manager_address")?,
                            wait_for_finalization: *required(&v1_conf.wait_for_finalization)
                                .context("wait_for_finalization")?,
                            settlement_layer_confirmation_depth: *required(
                                &v1_conf.settlement_layer_confirmation_depth,
                            )
                            .context("settlement_layer_confirmation_depth")?,
                            points_source: match v1_conf.points_source.clone() {
                                Some(proto::v1::PointsSource::PointsSourcePath(
                                    points_source_path,
                                )) => {
                                    zksync_config::configs::da_client::eigenda::PointsSource::Path(
                                        points_source_path,
                                    )
                                }
                                Some(proto::v1::PointsSource::PointsSourceUrl(
                                    points_source_url,
                                )) => {
                                    let g1_url =
                                        required(&points_source_url.g1_url).context("g1_url")?;
                                    let g2_url =
                                        required(&points_source_url.g2_url).context("g2_url")?;
                                    zksync_config::configs::da_client::eigenda::PointsSource::Url((
                                        g1_url.to_owned(),
                                        g2_url.to_owned(),
                                    ))
                                }
                                None => {
                                    return Err(anyhow::anyhow!("Invalid EigenDA configuration"))
                                }
                            },
                        })
                    }
                    Some(proto::eigen_da_config::ClientVersion::V2(v2_conf)) => {
                        VersionSpecificConfig::V2(V2Config {
                            cert_verifier_addr: required(&v2_conf.cert_verifier_addr)
                                .and_then(|x| parse_h160(x))
                                .context("eigenda_cert_and_blob_verifier_addr")?,
                            blob_version: *required(&v2_conf.blob_version)
                                .context("blob_version")?
                                as u16,
                            polynomial_form: match required(&v2_conf.polynomial_form)
                                .and_then(|x| {
                                    Ok(crate::proto::da_client::PolynomialForm::try_from(*x)?)
                                })
                                .context("polynomial_form")?
                            {
                                crate::proto::da_client::PolynomialForm::Coeff => {
                                    configs::da_client::eigenda::PolynomialForm::Coeff
                                }
                                crate::proto::da_client::PolynomialForm::Eval => {
                                    configs::da_client::eigenda::PolynomialForm::Eval
                                }
                            },
                        })
                    }
                    None => return Err(anyhow::anyhow!("Invalid EigenDA configuration")),
                },
            }),
            proto::data_availability_client::Config::ObjectStore(conf) => {
                ObjectStore(object_store_proto::ObjectStore::read(conf)?)
            }
            proto::data_availability_client::Config::NoDa(_) => NoDA,
        };

        Ok(client)
    }

    fn build(this: &Self::Type) -> Self {
        let config = match &this {
            Avail(config) => proto::data_availability_client::Config::Avail(proto::AvailConfig {
                bridge_api_url: Some(config.bridge_api_url.clone()),
                timeout_ms: Some(config.timeout_ms as u64),
                config: match &config.config {
                    AvailClientConfig::FullClient(conf) => Some(
                        proto::avail_config::Config::FullClient(proto::AvailClientConfig {
                            api_node_url: Some(conf.api_node_url.clone()),
                            app_id: Some(conf.app_id),
                            finality_state: conf.finality_state.clone(),
                            dispatch_timeout_ms: conf.dispatch_timeout_ms,
                        }),
                    ),
                    AvailClientConfig::GasRelay(conf) => Some(
                        proto::avail_config::Config::GasRelay(proto::AvailGasRelayConfig {
                            gas_relay_api_url: Some(conf.gas_relay_api_url.clone()),
                            max_retries: Some(conf.max_retries as u64),
                        }),
                    ),
                },
            }),
            Celestia(config) => {
                proto::data_availability_client::Config::Celestia(proto::CelestiaConfig {
                    api_node_url: Some(config.api_node_url.clone()),
                    namespace: Some(config.namespace.clone()),
                    chain_id: Some(config.chain_id.clone()),
                    timeout_ms: Some(config.timeout_ms),
                })
            }
            EigenDA(config) => {
                proto::data_availability_client::Config::Eigenda(proto::EigenDaConfig {
                    disperser_rpc: Some(config.disperser_rpc.clone()),
                    eigenda_eth_rpc: config
                        .eigenda_eth_rpc
                        .as_ref()
                        .map(|a| a.expose_str().to_string()),
                    authenticated: Some(config.authenticated),
                    client_version: Some(match &config.version_specific {
                        zksync_config::configs::da_client::eigenda::VersionSpecificConfig::V1(
                            v1_conf,
                        ) => proto::eigen_da_config::ClientVersion::V1(proto::V1 {
                            settlement_layer_confirmation_depth: Some(
                                v1_conf.settlement_layer_confirmation_depth,
                            ),
                            eigenda_svc_manager_address: Some(format!(
                                "{:?}",
                                v1_conf.eigenda_svc_manager_address
                            )),
                            wait_for_finalization: Some(v1_conf.wait_for_finalization),
                            points_source: Some(match &v1_conf.points_source {
                                zksync_config::configs::da_client::eigenda::PointsSource::Path(
                                    path,
                                ) => proto::v1::PointsSource::PointsSourcePath(path.clone()),
                                zksync_config::configs::da_client::eigenda::PointsSource::Url(
                                    (g1_url, g2_url),
                                ) => proto::v1::PointsSource::PointsSourceUrl(proto::Url {
                                    g1_url: Some(g1_url.clone()),
                                    g2_url: Some(g2_url.clone()),
                                }),
                            }),
                            // We need to cast as u32 because proto doesn't support u8
                            custom_quorum_numbers: v1_conf
                                .custom_quorum_numbers
                                .iter()
                                .map(|x| *x as u32)
                                .collect(),
                        }),
                        zksync_config::configs::da_client::eigenda::VersionSpecificConfig::V2(
                            v2_conf,
                        ) => proto::eigen_da_config::ClientVersion::V2(proto::V2 {
                            cert_verifier_addr: Some(format!("{:?}", v2_conf.cert_verifier_addr)),
                            blob_version: Some(v2_conf.blob_version as u32),
                            polynomial_form: Some(match v2_conf.polynomial_form {
                                configs::da_client::eigenda::PolynomialForm::Coeff => {
                                    crate::proto::da_client::PolynomialForm::Coeff.into()
                                }
                                configs::da_client::eigenda::PolynomialForm::Eval => {
                                    crate::proto::da_client::PolynomialForm::Eval.into()
                                }
                            }),
                        }),
                    }),
                })
            }
            ObjectStore(config) => proto::data_availability_client::Config::ObjectStore(
                object_store_proto::ObjectStore::build(config),
            ),
            NoDA => proto::data_availability_client::Config::NoDa(proto::NoDaConfig {}),
        };

        Self {
            config: Some(config),
        }
    }
}
