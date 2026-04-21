import * as fs from 'fs';

export const ARTIFACTS_PATH = '../../../contracts/l1-contracts/out';
export const SYSTEM_ARTIFACTS_PATH = '../../../contracts/system-contracts/zkout';
// Read contract artifacts
function readContract(path: string, fileName: string, contractName?: string) {
    contractName = contractName || fileName;
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${contractName}.json`, { encoding: 'utf-8' }));
}
export const ArtifactL1BridgeHub = readContract(`${ARTIFACTS_PATH}`, 'L1Bridgehub');
export const ArtifactNativeTokenVault = readContract(`${ARTIFACTS_PATH}`, 'L2NativeTokenVault');
export const ArtifactL1NativeTokenVault = readContract(`${ARTIFACTS_PATH}`, 'L1NativeTokenVault');
export const ArtifactL1AssetRouter = readContract(`${ARTIFACTS_PATH}`, 'L1AssetRouter');
export const ArtifactL1AssetTracker = readContract(`${ARTIFACTS_PATH}`, 'L1AssetTracker');
