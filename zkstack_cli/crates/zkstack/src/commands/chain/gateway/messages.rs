use super::gateway_common::{GatewayMigrationProgressState, MigrationDirection};

pub(super) fn message_for_gateway_migration_progress_state(
    state: GatewayMigrationProgressState,
    direction: MigrationDirection,
) -> String {
    match state {
        GatewayMigrationProgressState::NotStarted => {
            "Notification has not yet been sent. Please use the command to send notification first.".to_string()
        }
        GatewayMigrationProgressState::NotificationSent => {
            "Notification has been sent, but the server has not yet picked it up. Please wait".to_string()
        }
        GatewayMigrationProgressState::NotificationReceived(substate) => {
            format!("The server has received the notification about the migration, but it needs to finish all outstanding transactions: {}. Please wait", substate)
        }
        GatewayMigrationProgressState::ServerReady => {
            "The server is ready to start the migration. Please use the command to prepare the calldata and execute it inside your ChainAdmin".to_string()
        }
        GatewayMigrationProgressState::AwaitingFinalization => {
            match direction {
                MigrationDirection::FromGateway => {
                    "The transaction to migrate chain from Gateway has been processed on the ZK Gateway, but the ZK Gateway has not yet finalized it".to_string()
                },
                MigrationDirection::ToGateway => {
                    "The transaction to migrate chain on top of Gateway has been processed on L1, but the server has not yet started settling on top of Gateway".to_string()
                }
            }
        }
        GatewayMigrationProgressState::PendingManualFinalization => {
            "The chain migration from Gateway to L1 has been finalized on the Gateway side. Please use the command to finalize the migration to complete the process".to_string()
        }
        GatewayMigrationProgressState::Finished => {
            "The migration in this direction has been already finished".to_string()
        }
    }
}

pub(super) const USE_SET_DA_VALIDATOR_COMMAND_INFO: &str =
    "To prepare the calldata to set the DA validator pair please use the corresponding command";
