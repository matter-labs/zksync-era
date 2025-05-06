import * as fs from 'fs';
import { getConfigPath } from 'utils/src/file-configs';
import * as yaml from 'yaml';

export function setSnapshotRecovery(pathToHome: string, chain: string): ReverseConfigPatch {
    return setPropertiesInGeneralConfig(pathToHome, chain, { 'snapshot_recovery.enabled': true });
}

export function setTreeRecoveryParallelPersistenceBuffer(
    pathToHome: string,
    chain: string,
    value: number
): ReverseConfigPatch {
    return setPropertiesInGeneralConfig(pathToHome, chain, {
        'snapshot_recovery.experimental.tree_recovery_parallel_persistence_buffer': value
    });
}

interface PruningOptions {
    readonly chunkSize: number;
    readonly dataRetentionSec: number;
    readonly removalDelaySec: number;
}

export function setPruning(pathToHome: string, chain: string, options: PruningOptions): ReverseConfigPatch {
    return setPropertiesInGeneralConfig(pathToHome, chain, {
        'pruning.enabled': true,
        'pruning.chunk_size': options.chunkSize,
        'pruning.data_retention_sec': options.dataRetentionSec,
        'pruning.removal_delay_sec': options.removalDelaySec
    });
}

type ConfigProperties = Record<string, any>;

/** Patch than reverses application of changes in a config. */
export class ReverseConfigPatch {
    constructor(
        private readonly pathToHome: string,
        private readonly chain: string,
        private readonly properties: ConfigProperties
    ) {}

    reverse() {
        setPropertiesInGeneralConfig(this.pathToHome, this.chain, this.properties, true);
    }
}

function setPropertiesInGeneralConfig(
    pathToHome: string,
    chain: string,
    properties: ConfigProperties,
    isReverse: boolean = false
): ReverseConfigPatch {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain,
        configsFolder: 'configs/external_node',
        config: 'general.yaml'
    });
    let generalConfig = yaml.parseDocument(fs.readFileSync(generalConfigPath, 'utf8'));

    const reverseProperties: ConfigProperties = {};
    for (const [property, value] of Object.entries(properties)) {
        const path = property.split('.');
        const parentPath = path.slice(0, -1);
        if (!generalConfig.hasIn(parentPath)) {
            throw new Error(`Config at ${generalConfigPath} doesn't contain parent for property ${property}`);
        }

        reverseProperties[property] = generalConfig.getIn(path);

        if (value === undefined) {
            generalConfig.deleteIn(path);
        } else {
            generalConfig.setIn(path, value);
        }
    }

    fs.writeFileSync(generalConfigPath, generalConfig.toString(), 'utf8');
    if (isReverse) {
        console.log(`Reversed properties in ${generalConfigPath} to previous values:`, properties);
    } else {
        console.log(`Created reverse patch for ${generalConfigPath}:`, reverseProperties);
    }
    return new ReverseConfigPatch(pathToHome, chain, reverseProperties);
}

export function readContract(path: string, fileName: string) {
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${fileName}.json`, { encoding: 'utf-8' }));
}
