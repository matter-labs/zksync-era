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
