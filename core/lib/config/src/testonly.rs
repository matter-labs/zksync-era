use rand::{distributions::Distribution, Rng};
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    L1ChainId, L2ChainId,
};
use zksync_consensus_utils::EncodeDist;
use zksync_crypto_primitives::K256PrivateKey;

use crate::{
    configs::{
        self,
        da_client::{
            avail::{AvailClientConfig, AvailDefaultConfig},
            DAClientConfig::Avail,
        },
    },
    AvailConfig,
};

impl Distribution<configs::GenesisConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::GenesisConfig {
        configs::GenesisConfig {
            protocol_version: Some(ProtocolSemanticVersion {
                minor: ProtocolVersionId::try_from(
                    rng.gen_range(0..(ProtocolVersionId::latest() as u16)),
                )
                .unwrap(),
                patch: VersionPatch(rng.gen()),
            }),
            genesis_root_hash: Some(rng.gen()),
            rollup_last_leaf_index: Some(self.sample(rng)),
            genesis_commitment: Some(rng.gen()),
            bootloader_hash: Some(rng.gen()),
            default_aa_hash: Some(rng.gen()),
            evm_emulator_hash: Some(rng.gen()),
            fee_account: rng.gen(),
            l1_chain_id: L1ChainId(self.sample(rng)),
            sl_chain_id: None,
            l2_chain_id: L2ChainId::default(),
            snark_wrapper_vk_hash: rng.gen(),
            dummy_verifier: rng.gen(),
            l1_batch_commit_data_generator_mode: match rng.gen_range(0..2) {
                0 => L1BatchCommitmentMode::Rollup,
                _ => L1BatchCommitmentMode::Validium,
            },
        }
    }
}

impl Distribution<configs::wallets::Wallet> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallet {
        configs::wallets::Wallet::new(K256PrivateKey::from_bytes(rng.gen()).unwrap())
    }
}

impl Distribution<configs::wallets::AddressWallet> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::AddressWallet {
        configs::wallets::AddressWallet::from_address(rng.gen())
    }
}

impl Distribution<configs::wallets::StateKeeper> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::StateKeeper {
        configs::wallets::StateKeeper {
            fee_account: self.sample(rng),
        }
    }
}

impl Distribution<configs::wallets::EthSender> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::EthSender {
        configs::wallets::EthSender {
            operator: self.sample(rng),
            blob_operator: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::wallets::TokenMultiplierSetter> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::TokenMultiplierSetter {
        configs::wallets::TokenMultiplierSetter {
            wallet: self.sample(rng),
        }
    }
}

impl Distribution<configs::wallets::Wallets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallets {
        configs::wallets::Wallets {
            state_keeper: self.sample_opt(|| self.sample(rng)),
            eth_sender: self.sample_opt(|| self.sample(rng)),
            token_multiplier_setter: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::da_client::DAClientConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::da_client::DAClientConfig {
        Avail(AvailConfig {
            bridge_api_url: self.sample(rng),
            timeout_ms: self.sample(rng),
            config: AvailClientConfig::FullClient(AvailDefaultConfig {
                api_node_url: self.sample(rng),
                app_id: self.sample(rng),
            }),
        })
    }
}
