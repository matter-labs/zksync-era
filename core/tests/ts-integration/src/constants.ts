import * as fs from 'fs';
// eslint-disable-next-line @typescript-eslint/no-var-requires
export const REQUIRED_L2_GAS_PRICE_PER_PUBDATA = 800;

export const SYSTEM_UPGRADE_L2_TX_TYPE = 254;
export const GATEWAY_CHAIN_ID = 506;
export const ADDRESS_ONE = '0x0000000000000000000000000000000000000001';
export const ETH_ADDRESS_IN_CONTRACTS = ADDRESS_ONE;
export const L1_TO_L2_ALIAS_OFFSET = '0x1111000000000000000000000000000000001111';
export const L2_BRIDGEHUB_ADDRESS = '0x0000000000000000000000000000000000010002';
export const L2_ASSET_ROUTER_ADDRESS = '0x0000000000000000000000000000000000010003';
export const L2_NATIVE_TOKEN_VAULT_ADDRESS = '0x0000000000000000000000000000000000010004';
export const L2_MESSAGE_ROOT_ADDRESS = '0x0000000000000000000000000000000000010005';
export const L2_INTEROP_ROOT_STORAGE_ADDRESS = '0x0000000000000000000000000000000000010008';
export const L2_MESSAGE_VERIFICATION_ADDRESS = '0x0000000000000000000000000000000000010009';
export const L2_CHAIN_ASSET_HANDLER_ADDRESS = '0x000000000000000000000000000000000001000A';
export const L2_INTEROP_CENTER_ADDRESS = '0x000000000000000000000000000000000001000C';
export const L2_INTEROP_HANDLER_ADDRESS = '0x000000000000000000000000000000000001000D';
export const L2_ASSET_TRACKER_ADDRESS = '0x000000000000000000000000000000000001000E';

// System contract addresses
export const SYSTEM_CONTEXT_ADDRESS = '0x000000000000000000000000000000000000800b';
export const DEPLOYER_SYSTEM_CONTRACT_ADDRESS = '0x0000000000000000000000000000000000008006';
export const L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR = '0x0000000000000000000000000000000000008008';
export const EMPTY_STRING_KECCAK = '0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470';
export const BRIDGEHUB_L2_CANONICAL_TRANSACTION_ABI =
    'tuple(uint256 txType, uint256 from, uint256 to, uint256 gasLimit, uint256 gasPerPubdataByteLimit, uint256 maxFeePerGas, uint256 maxPriorityFeePerGas, uint256 paymaster, uint256 nonce, uint256 value, uint256[4] reserved, bytes data, bytes signature, uint256[] factoryDeps, bytes paymasterInput, bytes reservedDynamic)';
export const BRIDGEHUB_L2_TRANSACTION_REQUEST_ABI =
    'tuple(address sender, address contractL2, uint256 mintValue, uint256 l2Value, bytes l2Calldata, uint256 l2GasLimit, uint256 l2GasPerPubdataByteLimit, bytes[] factoryDeps, address refundRecipient)';
export const L2_LOG_STRING =
    'tuple(uint8 l2ShardId,bool isService,uint16 txNumberInBatch,address sender,bytes32 key,bytes32 value)';
export const ARTIFACTS_PATH = '../../../contracts/l1-contracts/out';
export const SYSTEM_ARTIFACTS_PATH = '../../../contracts/system-contracts/zkout';
export const L1_ZK_ARTIFACTS_PATH = '../../../contracts/l1-contracts/zkout';

export const INTEROP_CALL_ABI =
    'tuple(bytes1 version, bool shadowAccount, address to, address from, uint256 value, bytes data)';
export const INTEROP_BUNDLE_ABI =
    'tuple(bytes1 version, uint256 sourceChainId, uint256 destinationChainId, bytes32 interopBundleSalt, tuple(bytes1 version, bool shadowAccount, address to, address from, uint256 value, bytes data)[] calls, (bytes executionAddress, bytes unbundlerAddress, bool useFixedFee) bundleAttributes)';

export const MESSAGE_INCLUSION_PROOF_ABI =
    'tuple(uint256 chainId, uint256 l1BatchNumber, uint256 l2MessageIndex, tuple(uint16 txNumberInBatch, address sender, bytes data) message, bytes32[] proof)';

// Read contract artifacts
function readContract(path: string, fileName: string, contractName?: string) {
    contractName = contractName || fileName;
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
}
export const ArtifactL1BridgeHub = readContract(`${ARTIFACTS_PATH}`, 'L1Bridgehub');
export const ArtifactInteropCenter = readContract(`${ARTIFACTS_PATH}`, 'InteropCenter');
export const ArtifactInteropHandler = readContract(`${ARTIFACTS_PATH}`, 'InteropHandler');
export const ArtifactL2InteropRootStorage = readContract(`${SYSTEM_ARTIFACTS_PATH}`, 'L2InteropRootStorage');
export const ArtifactL2MessageVerification = readContract(`${ARTIFACTS_PATH}`, 'L2MessageVerification');
export const ArtifactIERC7786Attributes = readContract(`${ARTIFACTS_PATH}`, 'IERC7786Attributes');
export const ArtifactNativeTokenVault = readContract(`${ARTIFACTS_PATH}`, 'L2NativeTokenVault');
export const ArtifactMintableERC20 = readContract(`${L1_ZK_ARTIFACTS_PATH}`, 'TestnetERC20Token');
export const ArtifactL1AssetRouter = readContract(`${ARTIFACTS_PATH}`, 'L1AssetRouter');
export const ArtifactL1AssetTracker = readContract(`${ARTIFACTS_PATH}`, 'L1AssetTracker');
export const ArtifactL2AssetTracker = readContract(`${ARTIFACTS_PATH}`, 'L2AssetTracker');
export const ArtifactDummyInteropRecipient = readContract(`${L1_ZK_ARTIFACTS_PATH}`, 'DummyInteropRecipient');
