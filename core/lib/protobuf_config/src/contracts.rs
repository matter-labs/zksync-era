use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{parse_h160, parse_h256, proto, repr::ProtoRepr};

impl proto::ProverAtGenesis {
    fn new(x: &configs::contracts::ProverAtGenesis) -> Self {
        use configs::contracts::ProverAtGenesis as From;
        match x {
            From::Fri => Self::Fri,
            From::Old => Self::Old,
        }
    }

    fn parse(&self) -> configs::contracts::ProverAtGenesis {
        use configs::contracts::ProverAtGenesis as To;
        match self {
            Self::Fri => To::Fri,
            Self::Old => To::Old,
        }
    }
}

impl ProtoRepr for proto::Contracts {
    type Type = configs::ContractsConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            governance_addr: required(&self.governance_addr)
                .and_then(|x| parse_h160(x))
                .context("governance_addr")?,
            mailbox_facet_addr: required(&self.mailbox_facet_addr)
                .and_then(|x| parse_h160(x))
                .context("mailbox_facet_addr")?,
            executor_facet_addr: required(&self.executor_facet_addr)
                .and_then(|x| parse_h160(x))
                .context("executor_facet_addr")?,
            admin_facet_addr: required(&self.admin_facet_addr)
                .and_then(|x| parse_h160(x))
                .context("admin_facet_addr")?,
            getters_facet_addr: required(&self.getters_facet_addr)
                .and_then(|x| parse_h160(x))
                .context("getters_facet_addr")?,
            verifier_addr: required(&self.verifier_addr)
                .and_then(|x| parse_h160(x))
                .context("verifier_addr")?,
            diamond_init_addr: required(&self.diamond_init_addr)
                .and_then(|x| parse_h160(x))
                .context("diamond_init_addr")?,
            diamond_upgrade_init_addr: required(&self.diamond_upgrade_init_addr)
                .and_then(|x| parse_h160(x))
                .context("diamond_upgrade_init_addr")?,
            diamond_proxy_addr: required(&self.diamond_proxy_addr)
                .and_then(|x| parse_h160(x))
                .context("diamond_proxy_addr")?,
            validator_timelock_addr: required(&self.validator_timelock_addr)
                .and_then(|x| parse_h160(x))
                .context("validator_timelock_addr")?,
            genesis_tx_hash: required(&self.genesis_tx_hash)
                .and_then(|x| parse_h256(x))
                .context("genesis_tx_hash")?,
            l1_erc20_bridge_proxy_addr: required(&self.l1_erc20_bridge_proxy_addr)
                .and_then(|x| parse_h160(x))
                .context("l1_erc20_bridge_proxy_addr")?,
            l1_erc20_bridge_impl_addr: required(&self.l1_erc20_bridge_impl_addr)
                .and_then(|x| parse_h160(x))
                .context("l1_erc20_bridge_impl_addr")?,
            l2_erc20_bridge_addr: required(&self.l2_erc20_bridge_addr)
                .and_then(|x| parse_h160(x))
                .context("l2_erc20_bridge_addr")?,
            l1_weth_bridge_proxy_addr: self
                .l1_weth_bridge_proxy_addr
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l1_weth_bridge_proxy_addr")?,
            l2_weth_bridge_addr: self
                .l2_weth_bridge_addr
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l2_weth_bridge_addr")?,
            l1_allow_list_addr: required(&self.l1_allow_list_addr)
                .and_then(|x| parse_h160(x))
                .context("l1_allow_list_addr")?,
            l2_testnet_paymaster_addr: self
                .l2_testnet_paymaster_addr
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("l2_testnet_paymaster_addr")?,
            recursion_scheduler_level_vk_hash: required(&self.recursion_scheduler_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_scheduler_level_vk_hash")?,
            recursion_node_level_vk_hash: required(&self.recursion_node_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_node_level_vk_hash")?,
            recursion_leaf_level_vk_hash: required(&self.recursion_leaf_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_leaf_level_vk_hash")?,
            recursion_circuits_set_vks_hash: required(&self.recursion_circuits_set_vks_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_circuits_set_vks_hash")?,
            l1_multicall3_addr: required(&self.l1_multicall3_addr)
                .and_then(|x| parse_h160(x))
                .context("l1_multicall3_addr")?,
            fri_recursion_scheduler_level_vk_hash: required(
                &self.fri_recursion_scheduler_level_vk_hash,
            )
            .and_then(|x| parse_h256(x))
            .context("fri_recursion_scheduler_level_vk_hash")?,
            fri_recursion_node_level_vk_hash: required(&self.fri_recursion_node_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("fri_recursion_node_level_vk_hash")?,
            fri_recursion_leaf_level_vk_hash: required(&self.fri_recursion_leaf_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("fri_recursion_leaf_level_vk_hash")?,
            prover_at_genesis: required(&self.prover_at_genesis)
                .and_then(|x| Ok(proto::ProverAtGenesis::try_from(*x)?))
                .context("prover_at_genesis")?
                .parse(),
            snark_wrapper_vk_hash: required(&self.snark_wrapper_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("snark_wrapper_vk_hash")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            governance_addr: Some(this.governance_addr.as_bytes().into()),
            mailbox_facet_addr: Some(this.mailbox_facet_addr.as_bytes().into()),
            executor_facet_addr: Some(this.executor_facet_addr.as_bytes().into()),
            admin_facet_addr: Some(this.admin_facet_addr.as_bytes().into()),
            getters_facet_addr: Some(this.getters_facet_addr.as_bytes().into()),
            verifier_addr: Some(this.verifier_addr.as_bytes().into()),
            diamond_init_addr: Some(this.diamond_init_addr.as_bytes().into()),
            diamond_upgrade_init_addr: Some(this.diamond_upgrade_init_addr.as_bytes().into()),
            diamond_proxy_addr: Some(this.diamond_proxy_addr.as_bytes().into()),
            validator_timelock_addr: Some(this.validator_timelock_addr.as_bytes().into()),
            genesis_tx_hash: Some(this.genesis_tx_hash.as_bytes().into()),
            l1_erc20_bridge_proxy_addr: Some(this.l1_erc20_bridge_proxy_addr.as_bytes().into()),
            l1_erc20_bridge_impl_addr: Some(this.l1_erc20_bridge_impl_addr.as_bytes().into()),
            l2_erc20_bridge_addr: Some(this.l2_erc20_bridge_addr.as_bytes().into()),
            l1_weth_bridge_proxy_addr: this
                .l1_weth_bridge_proxy_addr
                .as_ref()
                .map(|x| x.as_bytes().into()),
            l2_weth_bridge_addr: this
                .l2_weth_bridge_addr
                .as_ref()
                .map(|x| x.as_bytes().into()),
            l1_allow_list_addr: Some(this.l1_allow_list_addr.as_bytes().into()),
            l2_testnet_paymaster_addr: this
                .l2_testnet_paymaster_addr
                .as_ref()
                .map(|x| x.as_bytes().into()),
            recursion_scheduler_level_vk_hash: Some(
                this.recursion_scheduler_level_vk_hash.as_bytes().into(),
            ),
            recursion_node_level_vk_hash: Some(this.recursion_node_level_vk_hash.as_bytes().into()),
            recursion_leaf_level_vk_hash: Some(this.recursion_leaf_level_vk_hash.as_bytes().into()),
            recursion_circuits_set_vks_hash: Some(
                this.recursion_circuits_set_vks_hash.as_bytes().into(),
            ),
            l1_multicall3_addr: Some(this.l1_multicall3_addr.as_bytes().into()),
            fri_recursion_scheduler_level_vk_hash: Some(
                this.fri_recursion_scheduler_level_vk_hash.as_bytes().into(),
            ),
            fri_recursion_node_level_vk_hash: Some(
                this.fri_recursion_node_level_vk_hash.as_bytes().into(),
            ),
            fri_recursion_leaf_level_vk_hash: Some(
                this.fri_recursion_leaf_level_vk_hash.as_bytes().into(),
            ),
            prover_at_genesis: Some(proto::ProverAtGenesis::new(&this.prover_at_genesis).into()),
            snark_wrapper_vk_hash: Some(this.snark_wrapper_vk_hash.as_bytes().into()),
        }
    }
}
