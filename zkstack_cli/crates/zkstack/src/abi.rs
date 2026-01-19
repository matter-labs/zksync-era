use ethers::contract::abigen;

abigen!(
    BridgehubAbi,
    "../../../contracts/l1-contracts/zkstack-out/L1Bridgehub.sol/L1Bridgehub.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ZkChainAbi,
    "../../../contracts/l1-contracts/zkstack-out/IZKChain.sol/IZKChain.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ChainTypeManagerAbi,
    "../../../contracts/l1-contracts/zkstack-out/IChainTypeManagerForZKStack.sol/IChainTypeManagerForZKStack.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ValidatorTimelockAbi,
    "../../../contracts/l1-contracts/zkstack-out/IValidatorTimelockForZKStack.sol/IValidatorTimelockForZKStack.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IChainAssetHandlerAbi,
    "../../../contracts/l1-contracts/zkstack-out/IChainAssetHandler.sol/IChainAssetHandler.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

// Using IChainTypeManager for the upgradeChainFromVersion function
abigen!(
    ChainTypeManagerUpgradeFnAbi,
    "../../../contracts/l1-contracts/zkstack-out/IChainTypeManager.sol/IChainTypeManager.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

// Using IAdmin for the diamondCut function
abigen!(
    DiamondCutAbi,
    "../../../contracts/l1-contracts/zkstack-out/IAdmin.sol/IAdmin.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ChainAdminOwnableAbi,
    "../../../contracts/l1-contracts/zkstack-out/IChainAdminOwnable.sol/IChainAdminOwnable.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IRegisterZKChainAbi,
    "../../../contracts/l1-contracts/zkstack-out/IRegisterZKChain.sol/IRegisterZKChain.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IDeployL2ContractsAbi,
    "../../../contracts/l1-contracts/zkstack-out/IDeployL2Contracts.sol/IDeployL2Contracts.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    DeployPaymasterAbi,
    "../../../contracts/l1-contracts/zkstack-out/DeployPaymaster.s.sol/DeployPaymaster.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IDeployPaymasterAbi,
    "../../../contracts/l1-contracts/zkstack-out/IDeployPaymaster.sol/IDeployPaymaster.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    GatewayVotePreparationAbi,
    "../../../contracts/l1-contracts/zkstack-out/GatewayVotePreparation.s.sol/GatewayVotePreparation.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    AdminFunctionsAbi,
    "../../../contracts/l1-contracts/zkstack-out/AdminFunctions.s.sol/AdminFunctions.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    EnableEvmEmulatorAbi,
    "../../../contracts/l1-contracts/zkstack-out/EnableEvmEmulator.s.sol/EnableEvmEmulator.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    DeployGatewayTransactionFiltererAbi,
    "../../../contracts/l1-contracts/zkstack-out/IDeployGatewayTransactionFilterer.sol/IDeployGatewayTransactionFilterer.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    GatewayUtilsAbi,
    "../../../contracts/l1-contracts/zkstack-out/IGatewayUtils.sol/IGatewayUtils.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IDeployCTMAbi,
    "../../../contracts/l1-contracts/zkstack-out/IDeployCTM.sol/IDeployCTM.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IRegisterCTMAbi,
    "../../../contracts/l1-contracts/zkstack-out/IRegisterCTM.sol/IRegisterCTM.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IRegisterOnAllChainsAbi,
    "../../../contracts/l1-contracts/zkstack-out/IRegisterOnAllChains.sol/IRegisterOnAllChains.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IGatewayMigrateTokenBalancesAbi,
    "../../../contracts/l1-contracts/zkstack-out/IGatewayMigrateTokenBalances.sol/IGatewayMigrateTokenBalances.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IFinalizeUpgradeAbi,
    "../../../contracts/l1-contracts/zkstack-out/IFinalizeUpgrade.sol/IFinalizeUpgrade.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IL2NativeTokenVaultAbi,
    "../../../contracts/l1-contracts/zkstack-out/IL2NativeTokenVault.sol/IL2NativeTokenVault.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IL2AssetRouterAbi,
    "../../../contracts/l1-contracts/zkstack-out/IL2AssetRouter.sol/IL2AssetRouter.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IL2AssetTrackerAbi,
    "../../../contracts/l1-contracts/zkstack-out/IL2AssetTracker.sol/IL2AssetTracker.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IChainAdminAbi,
    "../../../contracts/l1-contracts/zkstack-out/IChainAdmin.sol/IChainAdmin.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ISetupLegacyBridgeAbi,
    "../../../contracts/l1-contracts/zkstack-out/ISetupLegacyBridge.sol/ISetupLegacyBridge.json",
    event_derives(serde::Deserialize, serde::Serialize)
);
