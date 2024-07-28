import * as fs from 'fs';
import { background } from 'utils';
import { getConfigPath } from 'utils/build/file-configs';

export function runServerInBackground({
    components,
    stdio,
    cwd,
    useZkInception
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    useZkInception?: boolean;
}) {
    let command = useZkInception
        ? 'zk_inception server'
        : 'cd $ZKSYNC_HOME && cargo run --bin zksync_server --release --';
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    background({ command, stdio, cwd });
}

export function setEthSenderSenderAggregatedBlockCommitDeadline(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'aggregated_block_commit_deadline', value);
}

export function setAggregatedBlockProveDeadline(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'aggregated_block_prove_deadline', value);
}

export function setAggregatedBlockExecuteDeadline(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'aggregated_block_execute_deadline', value);
}

export function setBlockCommitDeadlineMs(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'block_commit_deadline_ms', value);
}

function setPropertyInGeneralConfig(pathToHome: string, fileConfig: any, property: string, value: number) {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain: fileConfig.chain,
        configsFolder: 'configs',
        config: 'general.yaml'
    });
    const generalConfig = fs.readFileSync(generalConfigPath, 'utf8');
    const regex = new RegExp(`\\b${property}:\\s*\\d+`, 'g');
    const newGeneralConfig = generalConfig.replace(regex, `${property}: ${value}`);

    fs.writeFileSync(generalConfigPath, newGeneralConfig, 'utf8');
}
