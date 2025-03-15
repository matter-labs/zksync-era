use std::{collections::VecDeque, convert::TryFrom, sync::Arc, time::Duration};

use anyhow::Context as _;
use once_cell::sync::OnceCell;
use rand::Rng;
use tokio::time::timeout;
use zksync_eth_signer::PrivateKeySigner;
use zksync_test_contracts::TestContract;
use zksync_types::{Address, K256PrivateKey, L2ChainId, H256};
use zksync_web3_decl::client::{Client, L2};

use crate::{
    config::LoadtestConfig,
    corrupted_tx::CorruptedSigner,
    rng::{LoadtestRng, Random},
    sdk::{signer::Signer, Wallet, ZksNamespaceClient},
};

/// An alias to [`zksync::Wallet`] with HTTP client. Wrapped in `Arc` since
/// the client cannot be cloned due to limitations in jsonrpsee.
pub type SyncWallet = Arc<Wallet<PrivateKeySigner, Client<L2>>>;
pub type CorruptedSyncWallet = Arc<Wallet<CorruptedSigner, Client<L2>>>;

/// Thread-safe pool of the addresses of accounts used in the loadtest.
#[derive(Debug, Clone)]
pub struct AddressPool {
    addresses: Arc<Vec<Address>>,
}

impl AddressPool {
    pub fn new(addresses: Vec<Address>) -> Self {
        Self {
            addresses: Arc::new(addresses),
        }
    }

    /// Randomly chooses one of the addresses stored in the pool.
    pub fn random_address(&self, rng: &mut LoadtestRng) -> Address {
        let index = rng.gen_range(0..self.addresses.len());
        self.addresses[index]
    }
}

/// Credentials for a test account.
/// Currently we support only EOA accounts.
#[derive(Debug, Clone)]
pub struct AccountCredentials {
    /// Ethereum private key.
    pub eth_pk: K256PrivateKey,
    /// Ethereum address derived from the private key.
    pub address: Address,
}

impl Random for AccountCredentials {
    fn random(rng: &mut LoadtestRng) -> Self {
        let eth_pk = K256PrivateKey::random_using(rng);
        let address = eth_pk.address();

        Self { eth_pk, address }
    }
}

/// Type that contains the data required for the test wallet to operate.
#[derive(Debug, Clone)]
pub struct TestWallet {
    /// Pre-initialized wallet object.
    pub wallet: SyncWallet,
    /// Wallet with corrupted signer.
    pub corrupted_wallet: CorruptedSyncWallet,
    /// Contract bytecode and calldata to be used for sending `Execute` transactions.
    pub test_contract: &'static TestContract,
    /// Address of the deployed contract to be used for sending
    /// `Execute` transaction.
    pub deployed_contract_address: Arc<OnceCell<Address>>,
    /// RNG object derived from a common loadtest seed and the wallet private key.
    pub rng: LoadtestRng,
}

/// Pool of accounts to be used in the test.
/// Each account is represented as `zksync::Wallet` in order to provide convenient interface of interaction with ZKsync.
#[derive(Debug)]
pub struct AccountPool {
    /// Main wallet that will be used to initialize all the test wallets.
    pub master_wallet: SyncWallet,
    /// Collection of test wallets and their Ethereum private keys.
    pub accounts: VecDeque<TestWallet>,
    /// Pool of addresses of the test accounts.
    pub addresses: AddressPool,
}

impl AccountPool {
    /// Generates all the required test accounts and prepares `Wallet` objects.
    pub async fn new(config: &LoadtestConfig) -> anyhow::Result<Self> {
        let l2_chain_id = L2ChainId::try_from(config.l2_chain_id)
            .map_err(|err| anyhow::anyhow!("invalid L2 chain ID: {err}"))?;
        // Create a client for pinging the RPC.
        let client = Client::http(
            config
                .l2_rpc_address
                .parse()
                .context("invalid L2 RPC URL")?,
        )?
        .for_network(l2_chain_id.into())
        .report_config(false)
        .build();

        // Perform a health check: check whether ZKsync server is alive.
        let mut server_alive = false;
        for _ in 0usize..3 {
            if let Ok(Ok(_)) = timeout(Duration::from_secs(3), client.get_main_l1_contract()).await
            {
                server_alive = true;
                break;
            }
        }
        if !server_alive {
            anyhow::bail!("ZKsync server does not respond. Please check RPC address and whether server is launched");
        }

        let test_contract = TestContract::load_test();

        let master_wallet = {
            let eth_private_key: H256 = config
                .master_wallet_pk
                .parse()
                .context("cannot parse master wallet private key")?;
            let eth_private_key = K256PrivateKey::from_bytes(eth_private_key)?;
            let address = eth_private_key.address();
            let eth_signer = PrivateKeySigner::new(eth_private_key);
            let signer = Signer::new(eth_signer, address, l2_chain_id);
            Arc::new(Wallet::with_http_client(&config.l2_rpc_address, signer).unwrap())
        };

        let mut rng = LoadtestRng::new_generic(config.seed.clone());
        tracing::info!("Using RNG with master seed: {}", rng.seed_hex());

        let group_size = config.accounts_group_size;
        let accounts_amount = config.accounts_amount;
        anyhow::ensure!(
            group_size <= accounts_amount,
            "Accounts group size is expected to be less than or equal to accounts amount"
        );

        let mut accounts = VecDeque::with_capacity(accounts_amount);
        let mut addresses = Vec::with_capacity(accounts_amount);

        for i in (0..accounts_amount).step_by(group_size) {
            let range_end = (i + group_size).min(accounts_amount);
            // The next group shares the contract address.
            let deployed_contract_address = Arc::new(OnceCell::new());

            for _ in i..range_end {
                let eth_credentials = AccountCredentials::random(&mut rng);
                let private_key_bytes = eth_credentials.eth_pk.expose_secret().secret_bytes();
                let eth_signer = PrivateKeySigner::new(eth_credentials.eth_pk);
                let address = eth_credentials.address;
                let signer = Signer::new(eth_signer, address, l2_chain_id);

                let corrupted_eth_signer = CorruptedSigner::new(address);
                let corrupted_signer = Signer::new(corrupted_eth_signer, address, l2_chain_id);

                let wallet = Wallet::with_http_client(&config.l2_rpc_address, signer).unwrap();
                let corrupted_wallet =
                    Wallet::with_http_client(&config.l2_rpc_address, corrupted_signer).unwrap();

                addresses.push(wallet.address());
                let account = TestWallet {
                    wallet: Arc::new(wallet),
                    corrupted_wallet: Arc::new(corrupted_wallet),
                    test_contract,
                    deployed_contract_address: deployed_contract_address.clone(),
                    rng: rng.derive(private_key_bytes),
                };
                accounts.push_back(account);
            }
        }

        Ok(Self {
            master_wallet,
            accounts,
            addresses: AddressPool::new(addresses),
        })
    }
}
