use crate::{parse_h160, parse_h256};
use zksync_config::configs::wallets::{EthSender, StateKeeper, Wallet};
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::wallets as proto;

impl ProtoRepr for proto::Wallets {
    type Type = configs::wallets::Wallets;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let eth_sender = if self.operator.is_some() && self.blob_operator.is_some() {
            Some(EthSender {
                operator: Wallet::from_private_key(parse_h256(required(
                    &self.operator.clone().unwrap().private_key,
                )?)?),
                blob_operator: Wallet::from_private_key(parse_h256(required(
                    &self.blob_operator.clone().unwrap().private_key,
                )?)?),
            })
        } else {
            None
        };

        let state_keeper = if let Some(fee_account) = &self.fee_account {
            // TODO verify private_key and address
            Some(StateKeeper {
                fee_account: Wallet::from_address(parse_h160(required(&fee_account.address)?)?),
            })
        } else {
            None
        };

        Ok(Self::Type {
            eth_sender,
            state_keeper,
        })
    }

    fn build(this: &Self::Type) -> Self {
        let (operator, blob_operator) = if let Some(eth_sender) = &this.eth_sender {
            (
                Some(proto::Wallet {
                    address: Some(eth_sender.operator.address().as_bytes().to_vec()),
                    private_key: eth_sender
                        .operator
                        .private_key()
                        .map(|a| a.as_bytes().to_vec()),
                }),
                Some(proto::Wallet {
                    address: Some(eth_sender.blob_operator.address().as_bytes().to_vec()),
                    private_key: eth_sender
                        .blob_operator
                        .private_key()
                        .map(|a| a.as_bytes().to_vec()),
                }),
            )
        } else {
            (None, None)
        };

        let fee_account = if let Some(state_keeper) = &this.state_keeper {
            Some(proto::Wallet {
                address: Some(state_keeper.fee_account.address().as_bytes().to_vec()),
                private_key: state_keeper
                    .fee_account
                    .private_key()
                    .map(|a| a.as_bytes().to_vec()),
            })
        } else {
            None
        };

        Self {
            blob_operator,
            operator,
            fee_account,
        }
    }
}
