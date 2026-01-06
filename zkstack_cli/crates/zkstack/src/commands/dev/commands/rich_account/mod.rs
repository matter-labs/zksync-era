use args::RichAccountArgs;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::ZkStackConfig;
use zksync_basic_types::H256;
use zksync_types::L2ChainId;

use crate::commands::dev::messages::msg_rich_account_outro;
pub mod args;
use alloy::{
    primitives::U256,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};

sol! {
    #[sol(rpc)]
    contract BridgehubAbi {
        struct L2TransactionRequestDirect {
            uint256 chainId;
            uint256 mintValue;
            address l2Contract;
            uint256 l2Value;
            bytes l2Calldata;
            uint256 l2GasLimit;
            uint256 l2GasPerPubdataByteLimit;
            bytes[] factoryDeps;
            address refundRecipient;
        }

        function requestL2TransactionDirect(L2TransactionRequestDirect _request) returns (bytes32 canonicalTxHash);

        function baseTokenAssetId(uint256 _chainId) returns (bytes32);

        function assetRouter() returns (address);
    }
}

sol! {
    #[sol(rpc)]
    contract AssetRouterAbi {
        function ETH_TOKEN_ASSET_ID() returns (bytes32);

        function nativeTokenVault() returns (address);

    }
}

sol! {
    #[sol(rpc)]
    contract NativeTokenVaultAbi {
        function tokenAddress(bytes32 _assetId) returns (address);
    }
}

sol! {
    #[sol(rpc)]
    contract TestnetBaseTokenAbi {
        function mint(address _to, uint256 _amount);
        function approve(address _spender, uint256 _amount);
    }
}

pub async fn run(
    shell: &Shell,
    args: RichAccountArgs,
    chain_id: Option<L2ChainId>,
) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    let chain_config = ZkStackConfig::current_chain(shell)?;

    let contracts_config = chain_config.get_contracts_config()?;
    let bridgehub_address = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;

    let signer: PrivateKeySigner = args.l1_account_private_key.parse()?;

    // Instantiate a provider with the signer
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect_http(reqwest::Url::parse(&args.l1_rpc_url).unwrap());

    let gas_price = provider.get_gas_price().await?;

    let bridgehub = BridgehubAbi::new(bridgehub_address.0.into(), provider.clone());

    let actual_chain_id = chain_id.unwrap_or(chain_config.chain_id);

    let base_token_asset_id: U256 = bridgehub
        .baseTokenAssetId(actual_chain_id.as_u64().try_into().unwrap())
        .call()
        .await?
        .into();

    let asset_router = AssetRouterAbi::new(bridgehub.assetRouter().call().await?, provider.clone());

    let eth_token_asset_id: U256 = asset_router.ETH_TOKEN_ASSET_ID().call().await?.into();

    let mut tmp_bytes = [0u8; 32];
    args.amount.to_little_endian(&mut tmp_bytes);
    let amount = U256::from_le_bytes(tmp_bytes);
    println!("kl todo 1");

    let eth_is_base_token = base_token_asset_id == eth_token_asset_id;
    if !eth_is_base_token {
        println!("base token asset id: {:?}", base_token_asset_id);
        let ntv = NativeTokenVaultAbi::new(
            asset_router.nativeTokenVault().call().await?,
            provider.clone(),
        );
        let base_token = TestnetBaseTokenAbi::new(
            ntv.tokenAddress(base_token_asset_id.into()).call().await?,
            provider.clone(),
        );
        println!("base token address: {:?}", base_token.address().0);
        let mint_tx = base_token
            .mint(args.l2_account.0.into(), amount)
            .gas_price(gas_price * 2)
            .send()
            .await?;
        let mint_receipt = mint_tx.get_receipt().await?;
        if !mint_receipt.status() {
            anyhow::bail!("Mint transaction failed");
        }

        let approve_tx = base_token
            .approve(ntv.address().0.into(), amount)
            .gas_price(gas_price * 2)
            .send()
            .await?;
        let approve_receipt = approve_tx.get_receipt().await?;
        if !approve_receipt.status() {
            anyhow::bail!("Approve transaction failed");
        }
    }

    let request = BridgehubAbi::L2TransactionRequestDirect {
        chainId: actual_chain_id.as_u64().try_into().unwrap(),
        mintValue: amount,
        l2Contract: args.l2_account.0.into(),
        l2Value: 0.try_into().unwrap(),
        l2Calldata: Default::default(),
        l2GasLimit: 1_000_000.try_into().unwrap(),
        l2GasPerPubdataByteLimit: 800.try_into().unwrap(),
        factoryDeps: Default::default(),
        refundRecipient: args.l2_account.0.into(),
    };

    let value = if eth_is_base_token {
        amount
    } else {
        U256::from(0)
    };

    let tx = bridgehub
        .requestL2TransactionDirect(request)
        .value(value)
        .gas_price(gas_price * 2)
        .send()
        .await?;
    let receipt = tx.get_receipt().await?;
    logger::info(
        format!(
            "Transaction hash: {:?} ",
            H256::from(receipt.transaction_hash.0)
        )
        .as_str(),
    );

    if receipt.status() {
        logger::outro(msg_rich_account_outro(&format!("{:?}", args.l2_account)));
    } else {
        anyhow::bail!("Transaction failed");
    }
    Ok(())
}
