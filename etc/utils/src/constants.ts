import * as fs from 'fs';

export const GATEWAY_CHAIN_ID = 506;
export const L2_BRIDGEHUB_ADDRESS = '0x0000000000000000000000000000000000010002';
export const L2_NATIVE_TOKEN_VAULT_ADDRESS = '0x0000000000000000000000000000000000010004';
export const L2_INTEROP_CENTER_ADDRESS = '0x000000000000000000000000000000000001000D';
export const L2_ASSET_TRACKER_ADDRESS = '0x000000000000000000000000000000000001000F';
export const GW_ASSET_TRACKER_ADDRESS = '0x0000000000000000000000000000000000010010';

export const ARTIFACTS_PATH = '../../../contracts/l1-contracts/out';
export const SYSTEM_ARTIFACTS_PATH = '../../../contracts/system-contracts/zkout';

export const INTEROP_BUNDLE_ABI =
    'tuple(bytes1 version, uint256 sourceChainId, uint256 destinationChainId, bytes32 destinationBaseTokenAssetId, bytes32 interopBundleSalt, tuple(bytes1 version, bool shadowAccount, address to, address from, uint256 value, bytes data)[] calls, (bytes executionAddress, bytes unbundlerAddress, bool useFixedFee) bundleAttributes)';

export const MESSAGE_INCLUSION_PROOF_ABI =
    'tuple(uint256 chainId, uint256 l1BatchNumber, uint256 l2MessageIndex, tuple(uint16 txNumberInBatch, address sender, bytes data) message, bytes32[] proof)';

// Read contract artifacts
function readContract(path: string, fileName: string, contractName?: string) {
    contractName = contractName || fileName;
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
}
export const ArtifactL1BridgeHub = readContract(`${ARTIFACTS_PATH}`, 'L1Bridgehub');
export const ArtifactNativeTokenVault = readContract(`${ARTIFACTS_PATH}`, 'L2NativeTokenVault');
export const ArtifactL1NativeTokenVault = readContract(`${ARTIFACTS_PATH}`, 'L1NativeTokenVault');
export const ArtifactWrappedBaseToken = readContract(`${ARTIFACTS_PATH}`, 'L2WrappedBaseToken');
export const ArtifactL1AssetRouter = readContract(`${ARTIFACTS_PATH}`, 'L1AssetRouter');
export const ArtifactL1AssetTracker = readContract(`${ARTIFACTS_PATH}`, 'L1AssetTracker');
export const ArtifactIBridgehubBase = readContract(`${ARTIFACTS_PATH}`, 'IBridgehubBase');
export const ArtifactIGetters = readContract(`${ARTIFACTS_PATH}`, 'IGetters');
export const ArtifactGWAssetTracker = readContract(`${ARTIFACTS_PATH}`, 'GWAssetTracker');
