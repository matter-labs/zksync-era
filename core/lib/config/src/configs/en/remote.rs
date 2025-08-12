use serde::{Deserialize, Serialize};
use zksync_basic_types::{commitment::L1BatchCommitmentMode, Address};

use crate::configs::contracts::{
    chain::{ChainContracts, L2Contracts},
    ecosystem::{EcosystemCommonContracts, L1SpecificContracts},
    SettlementLayerSpecificContracts,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteENConfig {
    pub l1_bytecodes_supplier_addr: Option<Address>,
    pub l1_bridgehub_proxy_addr: Option<Address>,
    pub l1_state_transition_proxy_addr: Option<Address>,
    pub l1_diamond_proxy_addr: Address,
    // While on L1 shared bridge and legacy bridge are different contracts with different addresses,
    // the `l2_erc20_bridge_addr` and `l2_shared_bridge_addr` are basically the same contract, but with
    // a different name, with names adapted only for consistency.
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    /// Contract address that serves as a shared bridge on L2.
    /// It is expected that `L2SharedBridge` is used before gateway upgrade, and `L2AssetRouter` is used after.
    pub l2_shared_bridge_addr: Address,
    /// Address of `L2SharedBridge` that was used before gateway upgrade.
    /// `None` if chain genesis used post-gateway protocol version.
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    pub l1_erc20_bridge_proxy_addr: Option<Address>,
    pub l2_erc20_bridge_addr: Address,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l2_timestamp_asserter_addr: Option<Address>,
    pub l1_wrapped_base_token_store: Option<Address>,
    pub l1_server_notifier_addr: Option<Address>,
    pub l1_message_root_proxy_addr: Option<Address>,
    pub base_token_addr: Address,
    pub l2_multicall3: Option<Address>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub dummy_verifier: bool,
}

impl RemoteENConfig {
    pub fn l1_specific_contracts(&self) -> L1SpecificContracts {
        L1SpecificContracts {
            bytecodes_supplier_addr: self.l1_bytecodes_supplier_addr,
            wrapped_base_token_store: self.l1_wrapped_base_token_store,
            bridge_hub: self.l1_bridgehub_proxy_addr,
            shared_bridge: self.l1_shared_bridge_proxy_addr,
            message_root: self.l1_message_root_proxy_addr,
            erc_20_bridge: self.l1_erc20_bridge_proxy_addr,
            base_token_address: self.base_token_addr,
            server_notifier_addr: self.l1_server_notifier_addr,
            // We don't need chain admin for external node
            chain_admin: None,
        }
    }

    pub fn l1_settelment_contracts(&self) -> SettlementLayerSpecificContracts {
        SettlementLayerSpecificContracts {
            ecosystem_contracts: EcosystemCommonContracts {
                bridgehub_proxy_addr: self.l1_bridgehub_proxy_addr,
                state_transition_proxy_addr: self.l1_state_transition_proxy_addr,
                message_root_proxy_addr: self.l1_message_root_proxy_addr,
                // Multicall 3 is useless for external node
                multicall3: None,
                validator_timelock_addr: None,
            },
            chain_contracts_config: ChainContracts {
                diamond_proxy_addr: self.l1_diamond_proxy_addr,
            },
        }
    }

    pub fn l2_contracts(&self) -> L2Contracts {
        L2Contracts {
            erc20_default_bridge: self.l2_erc20_bridge_addr,
            shared_bridge_addr: self.l2_shared_bridge_addr,
            legacy_shared_bridge_addr: self.l2_legacy_shared_bridge_addr,
            timestamp_asserter_addr: self.l2_timestamp_asserter_addr,
            testnet_paymaster_addr: self.l2_testnet_paymaster_addr,
            multicall3: self.l2_multicall3,
            da_validator_addr: None,
        }
    }
}
