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
    function chainAssetHandler() external view returns (address)
    function owner()(address)
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
    function validatorTimelockPostV29()(address)
    function forwardedBridgeMint(uint256 _chainId,bytes calldata _ctmData)(address)
    function serverNotifierAddress()(address)
    function protocolVersion()(uint256)
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

abigen!(
    IChainAssetHandlerAbi,
    r"[
    event MigrationStarted(uint256 indexed chainId, bytes32 indexed assetId, uint256 indexed settlementLayerChainId)
    ]"
);

// These ABIs are defined in JSON format rather than human-readable form because
// ethers v2 cannot parse complex nested tuple parameters (e.g., tuple[] inside tuple)
// from the human-readable syntax

abigen!(
    ChainTypeManagerUpgradeFnAbi,
    r#"[
      {
        "type": "function",
        "name": "upgradeChainFromVersion",
        "stateMutability": "nonpayable",
        "inputs": [
          { "name": "version", "type": "uint256" },
          {
            "name": "cfg",
            "type": "tuple",
            "components": [
              {
                "name": "validators",
                "type": "tuple[]",
                "components": [
                  { "name": "addr", "type": "address" },
                  { "name": "weight", "type": "uint8" },
                  { "name": "active", "type": "bool" },
                  { "name": "selectors", "type": "bytes4[]" }
                ]
              },
              { "name": "admin", "type": "address" },
              { "name": "data", "type": "bytes" }
            ]
          }
        ],
        "outputs": []
      }
    ]"#
);

abigen!(
    DiamondCutAbi,
    r#"[
      {
        "type": "function",
        "name": "diamondCut",
        "stateMutability": "nonpayable",
        "inputs": [
          {
            "name": "_diamondCut",
            "type": "tuple",
            "components": [
              {
                "name": "facetCuts",
                "type": "tuple[]",
                "components": [
                  { "name": "facet", "type": "address" },
                  { "name": "action", "type": "uint8" },
                  { "name": "isFreezable", "type": "bool" },
                  { "name": "selectors", "type": "bytes4[]" }
                ]
              },
              { "name": "initAddress", "type": "address" },
              { "name": "initCalldata", "type": "bytes" }
            ]
          }
        ],
        "outputs": []
      }
    ]"#
);

abigen!(
    ChainAdminOwnableAbi,
    r#"[
      {
        "type": "function",
        "name": "multicall",
        "stateMutability": "payable",
        "inputs": [
          {
            "name": "_calls",
            "type": "tuple[]",
            "components": [
              { "name": "target", "type": "address" },
              { "name": "value",  "type": "uint256" },
              { "name": "data",   "type": "bytes" }
            ]
          },
          { "name": "_requireSuccess", "type": "bool" }
        ],
        "outputs": []
      }
    ]"#
);
