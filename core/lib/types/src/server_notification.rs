use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{settlement::SettlementLayer, H256};
use zksync_contracts::server_notifier_contract;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayMigrationState {
    InProgress,
    NotInProgress,
}

impl GatewayMigrationState {
    pub fn from_sl_and_notification(
        settlement_layer: Option<SettlementLayer>,
        notification: Option<GatewayMigrationNotification>,
    ) -> Self {
        let Some(settlement_layer) = settlement_layer else {
            return GatewayMigrationState::InProgress;
        };
        notification
            .map(|a| match (a, settlement_layer) {
                (GatewayMigrationNotification::ToGateway, SettlementLayer::L1(_))
                | (GatewayMigrationNotification::FromGateway, SettlementLayer::Gateway(_)) => {
                    GatewayMigrationState::InProgress
                }
                _ => GatewayMigrationState::NotInProgress,
            })
            .unwrap_or(GatewayMigrationState::NotInProgress)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayMigrationNotification {
    FromGateway,
    ToGateway,
}

pub static MIGRATE_FROM_GATEWAY_NOTIFICATION_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    let contract = server_notifier_contract();
    contract.event("MigrateFromGateway").unwrap().signature()
});

pub static MIGRATE_TO_GATEWAY_NOTIFICATION_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    let contract = server_notifier_contract();
    contract.event("MigrateToGateway").unwrap().signature()
});

impl GatewayMigrationNotification {
    pub fn get_server_notifier_topics() -> Vec<H256> {
        vec![
            *MIGRATE_FROM_GATEWAY_NOTIFICATION_SIGNATURE,
            *MIGRATE_TO_GATEWAY_NOTIFICATION_SIGNATURE,
        ]
    }

    pub fn from_topic(topic: H256) -> Option<Self> {
        if topic == *MIGRATE_FROM_GATEWAY_NOTIFICATION_SIGNATURE {
            Some(Self::FromGateway)
        } else if topic == *MIGRATE_TO_GATEWAY_NOTIFICATION_SIGNATURE {
            Some(Self::ToGateway)
        } else {
            None
        }
    }
}
