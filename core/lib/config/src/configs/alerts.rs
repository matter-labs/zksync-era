use serde::Deserialize;
use zksync_protobuf::ProtoFmt;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct AlertsConfig {
    /// List of panics' messages from external crypto code,
    /// that are sporadic and needed to be handled separately
    pub sporadic_crypto_errors_substrs: Vec<String>,
}

impl ProtoFmt for AlertsConfig {
    type Proto = super::proto::Alerts;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            sporadic_crypto_errors_substrs: r.sporadic_crypto_errors_substrs.clone(),
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            sporadic_crypto_errors_substrs: self.sporadic_crypto_errors_substrs.clone(),
        }
    }
}
