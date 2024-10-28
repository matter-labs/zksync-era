use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContractVerifierConfig {
    /// Max time of a single compilation (in s).
    pub compilation_timeout: u64,
    /// Port to which the Prometheus exporter server is listening.
    pub prometheus_port: u16,
    pub port: u16,
}

impl ContractVerifierConfig {
    pub fn compilation_timeout(&self) -> Duration {
        Duration::from_secs(self.compilation_timeout)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), self.port)
    }
}
