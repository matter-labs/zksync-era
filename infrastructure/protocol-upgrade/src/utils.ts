import fs from 'fs';
import { BytesLike } from 'ethers';

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
