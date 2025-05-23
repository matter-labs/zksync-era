use std::str::FromStr;

use anyhow::Context;
use zksync_config::{
    configs::{
        self,
        da_client::{
            avail::{AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig},
            celestia::CelestiaConfig,
            eigenv1m0::EigenConfigV1M0,
            eigenv2m1::EigenConfigV2M1,
            DAClientConfig::{Avail, Celestia, EigenV1M0, EigenV2M0, EigenV2M1, NoDA, ObjectStore},
        },
    },
    EigenConfigV2M0,
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
            proto::data_availability_client::Config::Eigenv1m0(conf) => {
                EigenV1M0(EigenConfigV1M0 {
                    disperser_rpc: required(&conf.disperser_rpc)
                        .context("disperser_rpc")?
                        .clone(),
                    settlement_layer_confirmation_depth: *required(
                        &conf.settlement_layer_confirmation_depth,
                    )
                    .context("settlement_layer_confirmation_depth")?,
                    eigenda_eth_rpc: conf
                        .eigenda_eth_rpc
                        .clone()
                        .map(|x| SensitiveUrl::from_str(&x).context("eigenda_eth_rpc"))
                        .transpose()
                        .context("eigenda_eth_rpc")?,
                    eigenda_svc_manager_address: required(&conf.eigenda_svc_manager_address)
                        .and_then(|x| parse_h160(x))
                        .context("eigenda_svc_manager_address")?,
                    wait_for_finalization: *required(&conf.wait_for_finalization)
                        .context("wait_for_finalization")?,
                    authenticated: *required(&conf.authenticated).context("authenticated")?,
                    points_source: match conf.points_source.clone() {
                        Some(proto::eigen_config_v1m0::PointsSource::PointsSourcePath(
                            points_source_path,
                        )) => zksync_config::configs::da_client::eigenv1m0::PointsSource::Path(
                            points_source_path,
                        ),
                        Some(proto::eigen_config_v1m0::PointsSource::PointsSourceUrl(
                            points_source_url,
                        )) => {
                            let g1_url = required(&points_source_url.g1_url).context("g1_url")?;
                            let g2_url = required(&points_source_url.g2_url).context("g2_url")?;
                            zksync_config::configs::da_client::eigenv1m0::PointsSource::Url((
                                g1_url.to_owned(),
                                g2_url.to_owned(),
                            ))
                        }
                        None => return Err(anyhow::anyhow!("Invalid Eigen DA configuration")),
                    },
                    custom_quorum_numbers: conf
                        .custom_quorum_numbers
                        .iter()
                        .map(|x| u8::try_from(*x).context("custom_quorum_numbers"))
                        .collect::<anyhow::Result<Vec<u8>>>()?,
                })
            }
            proto::data_availability_client::Config::Eigenv2m1(conf) => {
                EigenV2M1(EigenConfigV2M1 {
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
                    cert_verifier_addr: required(&conf.cert_verifier_addr)
                        .and_then(|x| parse_h160(x))
                        .context("eigenda_cert_verifier_addr")?,
                    blob_version: *required(&conf.blob_version).context("blob_version")? as u16,
                    polynomial_form: match required(&conf.polynomial_form)
                        .and_then(|x| Ok(crate::proto::da_client::PolynomialForm::try_from(*x)?))
                        .context("polynomial_form")?
                    {
                        crate::proto::da_client::PolynomialForm::Coeff => {
                            configs::da_client::eigenv2m1::PolynomialForm::Coeff
                        }
                        crate::proto::da_client::PolynomialForm::Eval => {
                            configs::da_client::eigenv2m1::PolynomialForm::Eval
                        }
                    },
                    eigenda_sidecar_rpc: required(&conf.eigenda_sidecar_rpc)
                        .context("eigenda_sidecar_rpc")?
                        .clone(),
                })
            }
            proto::data_availability_client::Config::Eigenv2m0(conf) => {
                EigenV2M0(EigenConfigV2M0 {
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
                    cert_verifier_addr: required(&conf.cert_verifier_addr)
                        .and_then(|x| parse_h160(x))
                        .context("eigenda_cert_verifier_addr")?,
                    blob_version: *required(&conf.blob_version).context("blob_version")? as u16,
                    polynomial_form: match required(&conf.polynomial_form)
                        .and_then(|x| Ok(crate::proto::da_client::PolynomialForm::try_from(*x)?))
                        .context("polynomial_form")?
                    {
                        crate::proto::da_client::PolynomialForm::Coeff => {
                            configs::da_client::eigenv2m0::PolynomialForm::Coeff
                        }
                        crate::proto::da_client::PolynomialForm::Eval => {
                            configs::da_client::eigenv2m0::PolynomialForm::Eval
                        }
                    },
                })
            }
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
            EigenV1M0(config) => {
                proto::data_availability_client::Config::Eigenv1m0(proto::EigenConfigV1m0 {
                    disperser_rpc: Some(config.disperser_rpc.clone()),
                    settlement_layer_confirmation_depth: Some(
                        config.settlement_layer_confirmation_depth,
                    ),
                    eigenda_eth_rpc: config
                        .eigenda_eth_rpc
                        .as_ref()
                        .map(|a| a.expose_str().to_string()),
                    eigenda_svc_manager_address: Some(format!(
                        "{:?}",
                        config.eigenda_svc_manager_address
                    )),
                    wait_for_finalization: Some(config.wait_for_finalization),
                    authenticated: Some(config.authenticated),
                    points_source: Some(match &config.points_source {
                        zksync_config::configs::da_client::eigenv1m0::PointsSource::Path(path) => {
                            proto::eigen_config_v1m0::PointsSource::PointsSourcePath(path.clone())
                        }
                        zksync_config::configs::da_client::eigenv1m0::PointsSource::Url((
                            g1_url,
                            g2_url,
                        )) => proto::eigen_config_v1m0::PointsSource::PointsSourceUrl(proto::Url {
                            g1_url: Some(g1_url.clone()),
                            g2_url: Some(g2_url.clone()),
                        }),
                    }),
                    // We need to cast as u32 because proto doesn't support u8
                    custom_quorum_numbers: config
                        .custom_quorum_numbers
                        .iter()
                        .map(|x| *x as u32)
                        .collect(),
                })
            }
            EigenV2M1(config) => {
                proto::data_availability_client::Config::Eigenv2m1(proto::EigenConfigV2m1 {
                    disperser_rpc: Some(config.disperser_rpc.clone()),
                    eigenda_eth_rpc: config
                        .eigenda_eth_rpc
                        .as_ref()
                        .map(|a| a.expose_str().to_string()),
                    authenticated: Some(config.authenticated),
                    cert_verifier_addr: Some(format!("{:?}", config.cert_verifier_addr)),
                    blob_version: Some(config.blob_version as u32),
                    polynomial_form: Some(match config.polynomial_form {
                        configs::da_client::eigenv2m1::PolynomialForm::Coeff => {
                            crate::proto::da_client::PolynomialForm::Coeff.into()
                        }
                        configs::da_client::eigenv2m1::PolynomialForm::Eval => {
                            crate::proto::da_client::PolynomialForm::Eval.into()
                        }
                    }),
                    eigenda_sidecar_rpc: Some(config.eigenda_sidecar_rpc.clone()),
                })
            }
            EigenV2M0(config) => {
                proto::data_availability_client::Config::Eigenv2m0(proto::EigenConfigV2m0 {
                    disperser_rpc: Some(config.disperser_rpc.clone()),
                    eigenda_eth_rpc: config
                        .eigenda_eth_rpc
                        .as_ref()
                        .map(|a| a.expose_str().to_string()),
                    authenticated: Some(config.authenticated),
                    cert_verifier_addr: Some(format!("{:?}", config.cert_verifier_addr)),
                    blob_version: Some(config.blob_version as u32),
                    polynomial_form: Some(match config.polynomial_form {
                        configs::da_client::eigenv2m0::PolynomialForm::Coeff => {
                            crate::proto::da_client::PolynomialForm::Coeff.into()
                        }
                        configs::da_client::eigenv2m0::PolynomialForm::Eval => {
                            crate::proto::da_client::PolynomialForm::Eval.into()
                        }
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
