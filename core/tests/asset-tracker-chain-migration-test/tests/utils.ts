import { ethers } from 'ethers';
import * as fs from 'fs';
import { getConfigPath } from 'utils/build/file-configs';

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
    chainTypeManager: any;
}

export function initContracts(pathToHome: string, zkStack: boolean): Contracts {
    if (zkStack) {
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
                require(`${CONTRACTS_FOLDER}/l1-contracts/zkout/L2ComplexUpgrader.sol/L2ComplexUpgrader.json`).abi
            ),
            counterBytecode: require(
                `${pathToHome}/core/tests/ts-integration/artifacts-zk/contracts/counter/counter.sol/Counter.json`
            ).deployedBytecode,
            chainTypeManager: new ethers.Interface(
                require(`${CONTRACTS_FOLDER}/l1-contracts/out/ChainTypeManager.sol/ChainTypeManager.json`).abi
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
                require(
                    `${pathToHome}/contracts/l2-contracts/zkout/ForceDeployUpgrader.sol/ForceDeployUpgrader.json`
                ).abi
            ),
            complexUpgraderAbi: new ethers.Interface(
                require(`${pathToHome}/contracts/l1-contracts/zkout/L2ComplexUpgrader.sol/L2ComplexUpgrader.json`).abi
            ),
            counterBytecode: require(`${pathToHome}/core/tests/ts-integration/zkout/counter.sol/Counter.json`)
                .deployedBytecode,
            chainTypeManager: new ethers.Interface(
                require(`${L1_CONTRACTS_FOLDER}/state-transition/ChainTypeManager.sol/ChainTypeManager.json`).abi
            )
        };
    }
}
