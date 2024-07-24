use anyhow::Context;
use zksync_config::configs::{
    self,
    wallets::{AddressWallet, BaseTokenAdjuster, EthSender, StateKeeper, Wallet},
};
use zksync_protobuf::{required, ProtoRepr};
use zksync_types::{Address, K256PrivateKey};

use crate::{parse_h160, parse_h256, proto::wallets as proto};

impl ProtoRepr for proto::Wallets {
    type Type = configs::wallets::Wallets;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let eth_sender = if self.operator.is_some() && self.blob_operator.is_some() {
            let blob_operator = if let Some(blob_operator) = &self.blob_operator {
                Some(Wallet::from_private_key_bytes(
                    parse_h256(required(&blob_operator.private_key).context("blob operator")?)?,
                    blob_operator
                        .address
                        .as_ref()
                        .and_then(|a| parse_h160(a).ok()),
                )?)
            } else {
                None
            };

            let operator_wallet = &self.operator.clone().context("Operator private key")?;

            let operator = Wallet::from_private_key_bytes(
                parse_h256(required(&operator_wallet.private_key).context("operator")?)?,
                operator_wallet
                    .address
                    .as_ref()
                    .and_then(|a| parse_h160(a).ok()),
            )?;

            Some(EthSender {
                operator,
                blob_operator,
            })
        } else {
            None
        };

        let state_keeper = if let Some(fee_account) = &self.fee_account {
            let address = parse_h160(
                required(&fee_account.address).context("fee_account.address requireed")?,
            )
            .context("fee_account.address")?;
            Some(StateKeeper {
                fee_account: AddressWallet::from_address(address),
            })
        } else {
            None
        };

        let base_token_adjuster = if let Some(governor) = &self.governor {
            let wallet = Wallet::from_private_key_bytes(
                parse_h256(required(&governor.private_key).context("governor")?)?,
                governor.address.as_ref().and_then(|a| parse_h160(a).ok()),
            )?;
            Some(BaseTokenAdjuster { wallet })
        } else {
            None
        };

        Ok(Self::Type {
            eth_sender,
            state_keeper,
            base_token_adjuster,
        })
    }

    fn build(this: &Self::Type) -> Self {
        let create_pk_wallet = |addr: Address, pk: &K256PrivateKey| -> proto::PrivateKeyWallet {
            proto::PrivateKeyWallet {
                address: Some(format!("{:?}", addr)),
                private_key: Some(hex::encode(pk.expose_secret().secret_bytes())),
            }
        };

        let (operator, blob_operator) = if let Some(eth_sender) = &this.eth_sender {
            let blob = eth_sender
                .blob_operator
                .as_ref()
                .map(|blob| create_pk_wallet(blob.address(), blob.private_key()));
            (
                Some(create_pk_wallet(
                    eth_sender.operator.address(),
                    eth_sender.operator.private_key(),
                )),
                blob,
            )
        } else {
            (None, None)
        };

        let fee_account = this
            .state_keeper
            .as_ref()
            .map(|state_keeper| proto::AddressWallet {
                address: Some(format!("{:?}", state_keeper.fee_account.address())),
            });

        let governor = if let Some(base_token_adjuster) = &this.base_token_adjuster {
            Some(create_pk_wallet(
                base_token_adjuster.wallet.address(),
                base_token_adjuster.wallet.private_key(),
            ))
        } else {
            None
        };

        Self {
            blob_operator,
            operator,
            fee_account,
            governor,
        }
    }
}
