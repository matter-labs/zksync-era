import fs from 'fs';
import { BytesLike, ethers } from 'ethers';
import { FacetCut } from 'l1-contracts/src.ts/diamondCut';
import { DiamondCutData } from './transaction';
import { DiamondInitFactory } from 'l1-contracts/typechain';

export const DEFAULT_UPGRADE_PATH = process.env.ZKSYNC_HOME + '/etc/upgrades';
export const DEFAULT_L2CONTRACTS_FOR_UPGRADE_PATH =
    process.env.ZKSYNC_HOME + '/contracts/l2-contracts/contracts/upgrades';
export const DEFAULT_L1CONTRACTS_FOR_UPGRADE_PATH =
    process.env.ZKSYNC_HOME + '/contracts/l1-contracts/contracts/upgrades';

export function getTimestampInSeconds() {
    return Math.floor(Date.now() / 1000);
}

export function getCryptoFileName(environment): string {
    return getUpgradePath(environment) + '/crypto.json';
}

export function getFacetCutsFileName(environment): string {
    return getUpgradePath(environment) + '/facetCuts.json';
}

export function getFacetsFileName(environment): string {
    return getUpgradePath(environment) + '/facets.json';
}

export function getL2UpgradeFileName(environment): string {
    return getUpgradePath(environment) + '/l2Upgrade.json';
}

export function getL2TransactionsFileName(environment): string {
    return getUpgradePath(environment) + '/transactions.json';
}

export function getPostUpgradeCalldataFileName(environment): string {
    return getUpgradePath(environment) + '/postUpgradeCalldata.json';
}

export function getGenesisFileName(environment): string {
    return getUpgradePath(environment) + '/genesis.json';
}

export function getNameOfTheLastUpgrade(): string {
    return fs.readdirSync(DEFAULT_UPGRADE_PATH).sort().reverse()[0];
}

export function getCommonDataFileName(): string {
    return getCommonUpgradePath() + '/common.json';
}

export function getCommonUpgradePath(): string {
    const currentUpgrade = getNameOfTheLastUpgrade();
    return `${DEFAULT_UPGRADE_PATH}/${currentUpgrade}/`;
}

export function getUpgradePath(environment: string): string {
    const upgradeEnvironment = environment ?? 'localhost';
    const path = `${getCommonUpgradePath()}${upgradeEnvironment}`;
    if (!fs.existsSync(path)) {
        fs.mkdirSync(path, { recursive: true });
    }
    return path;
}

export interface VerifierParams {
    recursionNodeLevelVkHash: BytesLike;
    recursionLeafLevelVkHash: BytesLike;
    recursionCircuitsSetVksHash: BytesLike;
}

// Bit shift by 32 does not work in JS, so we have to multiply by 2^32
export const SEMVER_MINOR_VERSION_MULTIPLIER = 4294967296;

// The major version is always 0 for now
export function packSemver(major: number, minor: number, patch: number) {
    if (major !== 0) {
        throw new Error('Major version must be 0');
    }

    return minor * SEMVER_MINOR_VERSION_MULTIPLIER + patch;
}

export function unpackStringSemVer(semver: string): [number, number, number] {
    const [major, minor, patch] = semver.split('.');
    return [parseInt(major), parseInt(minor), parseInt(patch)];
}

// Checks that the initial cut hash params are valid.
// Sometimes it makes sense to allow dummy values for testing purposes, but in production
// these values should be set correctly.
function checkValidInitialCutHashParams(
    facetCuts: FacetCut[],
    verifierParams: VerifierParams,
    l2BootloaderBytecodeHash: string,
    l2DefaultAccountBytecodeHash: string,
    verifier: string,
    blobVersionedHashRetriever: string,
    priorityTxMaxGasLimit: number
) {
    // We do not fetch the following numbers from the environment because they are very rarely changed
    // and we want to avoid the risk of accidentally changing them.
    const EXPECTED_FACET_CUTS = 4;
    const EXPECTED_PRIORITY_TX_MAX_GAS_LIMIT = 72_000_000;

    if (facetCuts.length != EXPECTED_FACET_CUTS) {
        throw new Error(`Expected ${EXPECTED_FACET_CUTS} facet cuts, got ${facetCuts.length}`);
    }

    if (verifierParams.recursionNodeLevelVkHash === ethers.constants.HashZero) {
        throw new Error('Recursion node level vk hash is zero');
    }
    if (verifierParams.recursionLeafLevelVkHash === ethers.constants.HashZero) {
        throw new Error('Recursion leaf level vk hash is zero');
    }
    if (verifierParams.recursionCircuitsSetVksHash !== ethers.constants.HashZero) {
        throw new Error('Recursion circuits set vks hash must be zero');
    }
    if (l2BootloaderBytecodeHash === ethers.constants.HashZero) {
        throw new Error('L2 bootloader bytecode hash is zero');
    }
    if (l2DefaultAccountBytecodeHash === ethers.constants.HashZero) {
        throw new Error('L2 default account bytecode hash is zero');
    }
    if (verifier === ethers.constants.AddressZero) {
        throw new Error('Verifier address is zero');
    }
    if (blobVersionedHashRetriever === ethers.constants.AddressZero) {
        throw new Error('Blob versioned hash retriever address is zero');
    }
    if (priorityTxMaxGasLimit !== EXPECTED_PRIORITY_TX_MAX_GAS_LIMIT) {
        throw new Error(
            `Expected priority tx max gas limit to be ${EXPECTED_PRIORITY_TX_MAX_GAS_LIMIT}, got ${priorityTxMaxGasLimit}`
        );
    }
}

export enum PubdataPricingMode {
    Rollup,
    Validium
}

// We should either reuse code or add a test for this function.
export function compileInitialCutHash(
    facetCuts: FacetCut[],
    verifierParams: VerifierParams,
    l2BootloaderBytecodeHash: string,
    l2DefaultAccountBytecodeHash: string,
    verifier: string,
    blobVersionedHashRetriever: string,
    priorityTxMaxGasLimit: number,
    diamondInit: string,
    strictMode: boolean = true
): DiamondCutData {
    if (strictMode) {
        checkValidInitialCutHashParams(
            facetCuts,
            verifierParams,
            l2BootloaderBytecodeHash,
            l2DefaultAccountBytecodeHash,
            verifier,
            blobVersionedHashRetriever,
            priorityTxMaxGasLimit
        );
    }

    const factory = new DiamondInitFactory();

    // FIXME: import these from SYSTME CONFIG
    const feeParams = {
        pubdataPricingMode: PubdataPricingMode.Rollup,
        batchOverheadL1Gas: 1000000,
        maxPubdataPerBatch: 120000,
        priorityTxMaxPubdata: 99000,
        maxL2GasPerBatch: 80000000,
        minimalL2GasPrice: 250000000
    };

    const diamondInitCalldata = factory.interface.encodeFunctionData('initialize', [
        // these first values are set in the contract
        {
            chainId: '0x0000000000000000000000000000000000000000000000000000000000000001',
            bridgehub: '0x0000000000000000000000000000000000001234',
            stateTransitionManager: '0x0000000000000000000000000000000000002234',
            protocolVersion: '0x0000000000000000000000000000000000002234',
            admin: '0x0000000000000000000000000000000000003234',
            validatorTimelock: '0x0000000000000000000000000000000000004234',
            baseToken: '0x0000000000000000000000000000000000004234',
            baseTokenBridge: '0x0000000000000000000000000000000000004234',
            storedBatchZero: '0x0000000000000000000000000000000000000000000000000000000000005432',
            verifier,
            verifierParams,
            l2BootloaderBytecodeHash,
            l2DefaultAccountBytecodeHash,
            priorityTxMaxGasLimit,
            feeParams,
            blobVersionedHashRetriever
        }
    ]);

    return {
        facetCuts,
        initAddress: diamondInit,
        initCalldata: '0x' + diamondInitCalldata.slice(2 + (4 + 9 * 32) * 2)
    };
}
