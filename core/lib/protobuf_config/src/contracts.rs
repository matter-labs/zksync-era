use anyhow::Context as _;
use zksync_config::configs::ContractsConfig;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, proto::contracts as proto};

impl ProtoRepr for proto::Contracts {
    type Type = ContractsConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let l1 = required(&self.l1).context("l1")?;
        let l2 = required(&self.l2).context("l2")?;
        let bridges = required(&self.bridges).context("bridges")?;
        let shared = required(&bridges.shared).context("shared")?;
        let erc20 = required(&bridges.erc20).context("erc20")?;
        let weth_bridge = required(&bridges.weth).context("weth_bridge")?;
        Ok(Self::Type {
            governance_addr: required(&l1.governance_addr)
                .and_then(|x| parse_h160(x))
                .context("governance_addr")?,
            verifier_addr: required(&l1.verifier_addr)
                .and_then(|x| parse_h160(x))
                .context("verifier_addr")?,
            default_upgrade_addr: required(&l1.default_upgrade_addr)
                .and_then(|x| parse_h160(x))
                .context("diamond_init_addr")?,
            diamond_proxy_addr: required(&l1.diamond_proxy_addr)
                .and_then(|x| parse_h160(x))
                .context("diamond_proxy_addr")?,
            validator_timelock_addr: required(&l1.validator_timelock_addr)
                .and_then(|x| parse_h160(x))
                .context("validator_timelock_addr")?,
            l1_erc20_bridge_proxy_addr: erc20
                .l1_address
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l1_erc20_bridge_proxy_addr")?,
            l2_erc20_bridge_addr: erc20
                .l2_address
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l1_erc20_bridge_impl_addr")?,
            l1_shared_bridge_proxy_addr: required(&shared.l1_address)
                .and_then(|x| parse_h160(x))
                .context("l1_shared_bridge_addr")?,
            l2_shared_bridge_addr: required(&shared.l2_address)
                .and_then(|x| parse_h160(x))
                .context("l2_shared_bridge_proxy_addr")?,
            l1_weth_bridge_proxy_addr: weth_bridge
                .l1_address
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l1_weth_bridge_addr")?,
            l2_weth_bridge_addr: weth_bridge
                .l2_address
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l2_weth_bridge_addr")?,
            l2_testnet_paymaster_addr: l2
                .testnet_paymaster_addr
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l2_testnet_paymaster_addr")?,
            l1_multicall3_addr: required(&l1.multicall3_addr)
                .and_then(|x| parse_h160(x))
                .context("l1_multicall3_addr")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            l1: Some(proto::L1 {
                governance_addr: Some(format!("{:?}", this.governance_addr)),
                verifier_addr: Some(format!("{:?}", this.verifier_addr)),
                diamond_proxy_addr: Some(format!("{:?}", this.diamond_proxy_addr)),
                validator_timelock_addr: Some(format!("{:?}", this.validator_timelock_addr)),
                default_upgrade_addr: Some(format!("{:?}", this.default_upgrade_addr)),
                multicall3_addr: Some(format!("{:?}", this.l1_multicall3_addr)),
            }),
            l2: Some(proto::L2 {
                testnet_paymaster_addr: this.l2_testnet_paymaster_addr.map(|a| format!("{:?}", a)),
            }),
            bridges: Some(proto::Bridges {
                shared: Some(proto::Bridge {
                    l1_address: Some(format!("{:?}", this.l1_shared_bridge_proxy_addr)),
                    l2_address: Some(format!("{:?}", this.l2_shared_bridge_addr)),
                }),
                erc20: Some(proto::Bridge {
                    l1_address: this.l1_erc20_bridge_proxy_addr.map(|a| format!("{:?}", a)),
                    l2_address: this.l2_erc20_bridge_addr.map(|a| format!("{:?}", a)),
                }),
                weth: Some(proto::Bridge {
                    l1_address: this.l1_weth_bridge_proxy_addr.map(|a| format!("{:?}", a)),
                    l2_address: this.l2_weth_bridge_addr.map(|a| format!("{:?}", a)),
                }),
            }),
        }
    }
}
