use ethers::contract::abigen;

abigen!(
    BridgehubAbi,
    "../../../contracts/l1-contracts/out/L1Bridgehub.sol/L1Bridgehub.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ZkChainAbi,
    "../../../contracts/l1-contracts/out/IZKChain.sol/IZKChain.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ChainTypeManagerAbi,
    "../../../contracts/l1-contracts/out/IChainTypeManagerForZKStack.sol/IChainTypeManagerForZKStack.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ValidatorTimelockAbi,
    "../../../contracts/l1-contracts/out/IValidatorTimelockForZKStack.sol/IValidatorTimelockForZKStack.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IChainAssetHandlerAbi,
    "../../../contracts/l1-contracts/out/IChainAssetHandler.sol/IChainAssetHandler.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

// Using IChainTypeManager for the upgradeChainFromVersion function
abigen!(
    ChainTypeManagerUpgradeFnAbi,
    "../../../contracts/l1-contracts/out/IChainTypeManager.sol/IChainTypeManager.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

// Using IAdmin for the diamondCut function
abigen!(
    DiamondCutAbi,
    "../../../contracts/l1-contracts/out/IAdmin.sol/IAdmin.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    ChainAdminOwnableAbi,
    "../../../contracts/l1-contracts/out/IChainAdminOwnable.sol/IChainAdminOwnable.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IRegisterZKChainAbi,
    "../../../contracts/l1-contracts/out/IRegisterZKChain.sol/IRegisterZKChain.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    IDeployL2ContractsAbi,
    "../../../contracts/l1-contracts/out/IDeployL2Contracts.sol/IDeployL2Contracts.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    DeployPaymasterAbi,
    "../../../contracts/l1-contracts/out/DeployPaymaster.s.sol/DeployPaymaster.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    GatewayVotePreparationAbi,
    "../../../contracts/l1-contracts/out/GatewayVotePreparation.s.sol/GatewayVotePreparation.json",
    event_derives(serde::Deserialize, serde::Serialize)
);
