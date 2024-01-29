use zksync_config::configs::AlertsConfig;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::Alerts {
    type Type = AlertsConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sporadic_crypto_errors_substrs: self.sporadic_crypto_errors_substrs.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sporadic_crypto_errors_substrs: this.sporadic_crypto_errors_substrs.clone(),
        }
    }
}
