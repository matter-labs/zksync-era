use std::{str::FromStr, sync::Arc};

use zksync_config::{
    configs::{
        contracts::chain::ProofManagerContracts,
        proof_manager::ProofManagerConfig,
        wallets::{Wallet, Wallets},
    },
    GasAdjusterConfig,
};
use zksync_contracts::proof_manager_contract;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::clients::{Client, DynClient, SigningClient, L2};
use zksync_eth_signer::PrivateKeySigner;
use zksync_node_fee_model::l1_gas_price::{GasAdjuster, GasAdjusterClient};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::{
    commitment::L1BatchCommitmentMode, pubdata_da::PubdataSendingMode, url::SensitiveUrl,
    L2ChainId, SLChainId,
};

use crate::{client::ProofManagerClient, ProofManager};

/// Wiring layer for proof manager.
#[derive(Debug)]
pub struct ProofManagerLayer {
    proof_manager_config: ProofManagerConfig,
    gas_adjuster_config: GasAdjusterConfig,
    proof_manager_contracts: ProofManagerContracts,
    wallets_config: Wallets,
    sl_chain_id: SLChainId,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    object_store: Arc<dyn ObjectStore>,
}

#[derive(IntoContext)]
pub struct Output {
    #[context(task)]
    proof_manager: ProofManager,
}

impl ProofManagerLayer {
    pub fn new(
        proof_manager_config: ProofManagerConfig,
        gas_adjuster_config: GasAdjusterConfig,
        proof_manager_contracts: ProofManagerContracts,
        wallets_config: Wallets,
        sl_chain_id: SLChainId,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            proof_manager_config,
            gas_adjuster_config,
            proof_manager_contracts,
            wallets_config,
            sl_chain_id,
            l2_chain_id,
        }
    }

    async fn create_client(
        &self,
        http_rpc_url: String,
        sl_chain_id: SLChainId,
        contracts: &ProofManagerContracts,
        owner_wallet: Wallet,
    ) -> ProofManagerClient {
        let operator_private_key = owner_wallet.private_key().clone();
        let operator_address = operator_private_key.address();
        let signer = PrivateKeySigner::new(operator_private_key);
        tracing::info!("Operator address: {operator_address:?}");

        let client = Box::new(
            Client::<L2>::http(SensitiveUrl::from_str(&http_rpc_url).expect("failed to parse url"))
                .expect("failed to create client")
                .for_network(L2::from(L2ChainId::from(sl_chain_id.0 as u32)))
                .build(),
        ) as Box<DynClient<L2>>;

        let gas_adjuster_client = GasAdjusterClient::from(client.clone_boxed());

        // todo: what is this?
        let gas_adjuster = Arc::new(
            GasAdjuster::new(
                gas_adjuster_client,
                self.gas_adjuster_config.clone(),
                PubdataSendingMode::Custom,
                L1BatchCommitmentMode::Rollup,
            )
            .await
            .unwrap(),
        );

        let eth_client = SigningClient::new(
            client,
            proof_manager_contract(),
            operator_address,
            signer,
            contracts.proxy_addr,
            self.proof_manager_config
                .default_priority_fee_per_gas
                .into(),
            self.sl_chain_id,
        );

        ProofManagerClient::new(
            Box::new(eth_client),
            gas_adjuster,
            self.proof_manager_config.clone(),
        )
    }
}

#[async_trait::async_trait]
impl WiringLayer for ProofManagerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "proof_manager_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;

        tracing::info!(
            "Proof manager address: {:#?}, proxy address: {:#?}",
            self.proof_manager_contracts.proof_manager_addr,
            self.proof_manager_contracts.proxy_addr
        );

        let client = self
            .create_client(
                self.proof_manager_config.http_rpc_url.clone(),
                self.sl_chain_id,
                &self.proof_manager_contracts,
                self.wallets_config
                    .proof_manager
                    .clone()
                    .expect("Eth proof manager wallet is required"),
            )
            .await;

        let public_object_store =
            ObjectStoreFactory::new(self.proof_manager_config.object_store.clone())
                .create_store()
                .await?;

        let proof_manager = ProofManager::new(
            Box::new(client),
            main_pool,
            input.object_store,
            public_object_store,
            self.proof_manager_config.clone(),
            self.l2_chain_id,
        );

        Ok(Output { proof_manager })
    }
}

#[async_trait::async_trait]
impl Task for ProofManager {
    fn id(&self) -> TaskId {
        "proof_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
