// use anyhow::Context as _;
// use zksync_config::configs::{ContractsConfig, EcosystemContracts, GatewayConfig};
// use zksync_protobuf::{repr::ProtoRepr, required};

// use crate::{parse_h160, proto::contracts as proto};

// impl ProtoRepr for proto::GatewayContractsConfig {
//     type Type = GatewayConfig;

//     fn read(&self) -> anyhow::Result<Self::Type> {

//         Ok(Self::Type {
//             state_transition_proxy_addr: required(&self.state_transition_proxy_addr)
//             .and_then(|x| parse_h160(x))
//             .context("state_transition_proxy_addr")?,
//         state_transition_implementation_addr: required(&self.state_transition_implementation_addr)
//             .and_then(|x| parse_h160(x))
//             .context("state_transition_implementation_addr")?,
//         verifier_addr: required(&self.verifier_addr)
//             .and_then(|x| parse_h160(x))
//             .context("verifier_addr")?,
//         admin_facet_addr: required(&self.admin_facet_addr)
//             .and_then(|x| parse_h160(x))
//             .context("admin_facet_addr")?,
//         mailbox_facet_addr: required(&self.mailbox_facet_addr)
//             .and_then(|x| parse_h160(x))
//             .context("mailbox_facet_addr")?,
//         executor_facet_addr: required(&self.executor_facet_addr)
//             .and_then(|x| parse_h160(x))
//             .context("executor_facet_addr")?,
//         getters_facet_addr: required(&self.getters_facet_addr)
//             .and_then(|x| parse_h160(x))
//             .context("getters_facet_addr")?,
//         diamond_init_addr: required(&self.diamond_init_addr)
//             .and_then(|x| parse_h160(x))
//             .context("diamond_init_addr")?,
//         genesis_upgrade_addr: required(&self.genesis_upgrade_addr)
//             .and_then(|x| parse_h160(x))
//             .context("genesis_upgrade_addr")?,
//         default_upgrade_addr: required(&self.default_upgrade_addr)
//             .and_then(|x| parse_h160(x))
//             .context("default_upgrade_addr")?,
//         diamond_cut_data: required(&self.diamond_cut_data)
//             .and_then(|x| parse_bytes(x))
//             .context("diamond_cut_data")?,
//         })
//     }

//     fn build(this: &Self::Type) -> Self {
//         let ecosystem_contracts = this
//             .ecosystem_contracts
//             .as_ref()
//             .map(|ecosystem_contracts| proto::EcosystemContracts {
//                 bridgehub_proxy_addr: Some(format!(
//                     "{:?}",
//                     ecosystem_contracts.bridgehub_proxy_addr
//                 )),
//                 state_transition_proxy_addr: Some(format!(
//                     "{:?}",
//                     ecosystem_contracts.state_transition_proxy_addr
//                 )),
//                 transparent_proxy_admin_addr: Some(format!(
//                     "{:?}",
//                     ecosystem_contracts.transparent_proxy_admin_addr,
//                 )),
//             });
//         Self {
//             ecosystem_contracts,
//             l1: Some(proto::L1 {
//                 governance_addr: Some(format!("{:?}", this.governance_addr)),
//                 verifier_addr: Some(format!("{:?}", this.verifier_addr)),
//                 diamond_proxy_addr: Some(format!("{:?}", this.diamond_proxy_addr)),
//                 validator_timelock_addr: Some(format!("{:?}", this.validator_timelock_addr)),
//                 default_upgrade_addr: Some(format!("{:?}", this.default_upgrade_addr)),
//                 multicall3_addr: Some(format!("{:?}", this.l1_multicall3_addr)),
//                 base_token_addr: this.base_token_addr.map(|a| format!("{:?}", a)),
//                 chain_admin_addr: this.chain_admin_addr.map(|a| format!("{:?}", a)),
//             }),
//             l2: Some(proto::L2 {
//                 testnet_paymaster_addr: this.l2_testnet_paymaster_addr.map(|a| format!("{:?}", a)),
//                 native_token_vault_addr: this
//                     .l2_native_token_vault_proxy_addr
//                     .map(|a| format!("{:?}", a)),
//                 da_validator_addr: this.l2_da_validator_addr.map(|a| format!("{:?}", a)),
//             }),
//             bridges: Some(proto::Bridges {
//                 shared: Some(proto::Bridge {
//                     l1_address: this.l1_shared_bridge_proxy_addr.map(|a| format!("{:?}", a)),
//                     l2_address: this.l2_shared_bridge_addr.map(|a| format!("{:?}", a)),
//                 }),
//                 erc20: Some(proto::Bridge {
//                     l1_address: this.l1_erc20_bridge_proxy_addr.map(|a| format!("{:?}", a)),
//                     l2_address: this.l2_erc20_bridge_addr.map(|a| format!("{:?}", a)),
//                 }),
//                 weth: Some(proto::Bridge {
//                     l1_address: this.l1_weth_bridge_proxy_addr.map(|a| format!("{:?}", a)),
//                     l2_address: this.l2_weth_bridge_addr.map(|a| format!("{:?}", a)),
//                 }),
//             }),
//             user_facing_bridgehub: this
//                 .user_facing_bridgehub_proxy_addr
//                 .map(|a| format!("{:?}", a)),
//             user_facing_diamond_proxy: this
//                 .user_facing_diamond_proxy_addr
//                 .map(|a| format!("{:?}", a)),
//         }
//     }
// }
