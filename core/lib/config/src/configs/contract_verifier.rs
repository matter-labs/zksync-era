use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ContractVerifierConfig {
    /// Max time of a single compilation (in s).
    #[config(default_t = Duration::from_secs(240), with = TimeUnit::Seconds)]
    pub compilation_timeout: Duration,
    /// Port to which the Prometheus exporter server is listening.
    #[config(default_t = 3_318)]
    pub prometheus_port: u16,
    #[config(default_t = 3_070)]
    pub port: u16,
}

impl ContractVerifierConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), self.port)
    }
}
