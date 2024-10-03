import { ethers } from 'ethers';
import * as fs from 'fs';
import { background } from 'utils';
import { getConfigPath } from 'utils/build/file-configs';

export function runServerInBackground({
    components,
    stdio,
    cwd,
    useZkInception,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    useZkInception?: boolean;
    chain?: string;
}) {
    let command = '';

    if (useZkInception) {
        command = 'zk_inception server';
        command += chain ? ` --chain ${chain}` : '';
    } else {
        command = 'cd $ZKSYNC_HOME && cargo run --bin zksync_server --release --';
    }
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

export interface Contracts {
    l1DefaultUpgradeAbi: any;
    governanceAbi: any;
    adminFacetAbi: any;
    chainAdminAbi: any;
    l2ForceDeployUpgraderAbi: any;
    complexUpgraderAbi: any;
    counterBytecode: any;
    stateTransitonManager: any;
}

export function initContracts(pathToHome: string, zkToolbox: boolean): Contracts {
    if (zkToolbox) {
        const CONTRACTS_FOLDER = `${pathToHome}/contracts`;
        return {
            l1DefaultUpgradeAbi: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l1-contracts/out/DefaultUpgrade.sol/DefaultUpgrade.json`).abi
            ),
            governanceAbi: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l1-contracts/out/Governance.sol/Governance.json`).abi
            ),
            adminFacetAbi: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l1-contracts/out/IAdmin.sol/IAdmin.json`).abi
            ),
            chainAdminAbi: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l1-contracts/out/ChainAdmin.sol/ChainAdmin.json`).abi
            ),
            l2ForceDeployUpgraderAbi: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l2-contracts/zkout/ForceDeployUpgrader.sol/ForceDeployUpgrader.json`).abi
            ),
            complexUpgraderAbi: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/system-contracts/zkout/ComplexUpgrader.sol/ComplexUpgrader.json`).abi
            ),
            counterBytecode:
                require(`${pathToHome}/core/tests/ts-integration/artifacts-zk/contracts/counter/counter.sol/Counter.json`)
                    .deployedBytecode,
            stateTransitonManager: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l1-contracts/out/StateTransitionManager.sol/StateTransitionManager.json`).abi
            )
        };
    } else {
        const L1_CONTRACTS_FOLDER = `${pathToHome}/contracts/l1-contracts/artifacts/contracts`;
        return {
            l1DefaultUpgradeAbi: new ethers.Interface(
                require(`${L1_CONTRACTS_FOLDER}/upgrades/DefaultUpgrade.sol/DefaultUpgrade.json`).abi
            ),
            governanceAbi: new ethers.Interface(
                require(`${L1_CONTRACTS_FOLDER}/governance/Governance.sol/Governance.json`).abi
            ),
            adminFacetAbi: new ethers.Interface(
                require(`${L1_CONTRACTS_FOLDER}/state-transition/chain-interfaces/IAdmin.sol/IAdmin.json`).abi
            ),
            chainAdminAbi: new ethers.Interface(
                require(`${L1_CONTRACTS_FOLDER}/governance/ChainAdmin.sol/ChainAdmin.json`).abi
            ),
            l2ForceDeployUpgraderAbi: new ethers.Interface(
                require(`${pathToHome}/contracts/l2-contracts/artifacts-zk/contracts/ForceDeployUpgrader.sol/ForceDeployUpgrader.json`).abi
            ),
            complexUpgraderAbi: new ethers.Interface(
                require(`${pathToHome}/contracts/system-contracts/artifacts-zk/contracts-preprocessed/ComplexUpgrader.sol/ComplexUpgrader.json`).abi
            ),
            counterBytecode:
                require(`${pathToHome}/core/tests/ts-integration/artifacts-zk/contracts/counter/counter.sol/Counter.json`)
                    .deployedBytecode,
            stateTransitonManager: new ethers.Interface(
                require(`${L1_CONTRACTS_FOLDER}/state-transition/StateTransitionManager.sol/StateTransitionManager.json`).abi
            )
        };
    }
}
