import * as fs from 'fs';
import { getConfigPath } from 'utils/build/file-configs';

export function setSnapshotRecovery(pathToHome: string, fileConfig: any, value: boolean) {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain: fileConfig.chain,
        configsFolder: 'configs/external_node',
        config: 'general.yaml'
    });
    const generalConfig = fs.readFileSync(generalConfigPath, 'utf8');
    // NOTE weak approach. It assumes the enabled property to be the first  within snapshot_recovery
    const regex = /(\bsnapshot_recovery:\s*\n\s*enabled:\s*)\w+/;
    const newGeneralConfig = generalConfig.replace(regex, `$1${value}`);

    fs.writeFileSync(generalConfigPath, newGeneralConfig, 'utf8');
}

export function setTreeRecoveryParallelPersistenceBuffer(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'tree_recovery_parallel_persistence_buffer', value);
}

export function setChunkSize(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'chunk_size', value);
}

export function setDataRetentionSec(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'data_retention_sec', value);
}

export function setRemovalDelaySec(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'removal_delay_sec', value);
}

function setPropertyInGeneralConfig(pathToHome: string, fileConfig: any, property: string, value: number) {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain: fileConfig.chain,
        configsFolder: 'configs/external_node',
        config: 'general.yaml'
    });
    const generalConfig = fs.readFileSync(generalConfigPath, 'utf8');
    const regex = new RegExp(`${property}:\\s*\\d+`, 'g');
    const newGeneralConfig = generalConfig.replace(regex, `${property}: ${value}`);

    fs.writeFileSync(generalConfigPath, newGeneralConfig, 'utf8');
}
