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
    function validatorTimelockPostV29()(address)
    function forwardedBridgeMint(uint256 _chainId,bytes calldata _ctmData)(address)
    function serverNotifierAddress()(address)
]"
);

// "validators" is from pre-v29, used for backward compatibility. Will be removed once v29 is released.
abigen!(
    ValidatorTimelockAbi,
    r"[
    function hasRole(uint256 _chainId, bytes32 _role, address _account)(bool)
    function hasRoleForChainId(uint256 _chainId, bytes32 _role, address _account)(bool)
    function PRECOMMITTER_ROLE()(bytes32)
    function COMMITTER_ROLE()(bytes32)
    function PROVER_ROLE()(bytes32)
    function EXECUTOR_ROLE()(bytes32)
    function validators(uint256 _chainId, address _validator)(bool)
    ]"
);
