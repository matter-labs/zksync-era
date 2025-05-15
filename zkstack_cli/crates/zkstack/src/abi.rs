use ethers::contract::abigen;

abigen!(
    BridgehubAbi,
    r"[
    function assetRouter()(address)
    function settlementLayer(uint256)(uint256)
    function getZKChain(uint256)(address)
    function ctmAssetIdToAddress(bytes32)(address)
    function ctmAssetIdFromChainId(uint256)(bytes32)
    function baseTokenAssetId(uint256)(bytes32)
    function chainTypeManager(uint256)(address)
]"
);

abigen!(
    ZkChainAbi,
    r"[
    function getDAValidatorPair()(address,address)
    function getAdmin()(address)
    function getProtocolVersion()(uint256)
    function getTotalBatchesCommitted()(uint256)
    function getTotalBatchesVerified()(uint256)
    function getTotalBatchesExecuted()(uint256)
]"
);

abigen!(
    ChainTypeManagerAbi,
    r"[
    function validatorTimelock()(address)
    function forwardedBridgeMint(uint256 _chainId,bytes calldata _ctmData)(address)
    function serverNotifierAddress()(address)
]"
);

abigen!(
    ValidatorTimelockAbi,
    r"[
    function validators(uint256 _chainId, address _validator)(bool)
]"
);
