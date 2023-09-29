import { Command } from 'commander';
import fs from 'fs';
import path from 'path';
import { ethers } from 'ethers';

// Command to verify the correctness of our protocol upgrades among all the environments.
//
// Currently it shows the summary of all the upgrades, and does the basic checks to make sure
// that the SystemContracts are assigned to the correct addresses, and that we pass correct information
// in Forced deployment calls.

// Things that it DOES NOT check yet are (these will come in future PRs):
// - facets
// - correctness of the bytecode hashes
// - that the final transaction's calldata is matching all the contents in the JSON files.

const UPGRADES_DIR = 'etc/upgrades';

// The first protocol version where launched the upgrade system was 12.
// So setting this constant to the protocol version before (11).
const LAST_PROTOCOL_VERSION_BEFORE_UPGRADE_SYSTEM = 11;

// Hardcoded list of system contract names to their corresponding addresses.
const SYSTEM_CONTRACT_MAP: Map<string, string> = new Map([
    ['AccountCodeStorage', '0x0000000000000000000000000000000000008002'],
    ['NonceHolder', '0x0000000000000000000000000000000000008003'],
    ['KnownCodesStorage', '0x0000000000000000000000000000000000008004'],
    ['ImmutableSimulator', '0x0000000000000000000000000000000000008005'],
    ['ContractDeployer', '0x0000000000000000000000000000000000008006'],
    // 0x8007 is a ForceDeployer address (which is not a system contract, but conventional address)
    ['L1Messenger', '0x0000000000000000000000000000000000008008'],
    ['MsgValueSimulator', '0x0000000000000000000000000000000000008009'],
    ['L2EthToken', '0x000000000000000000000000000000000000800a'],
    ['SystemContext', '0x000000000000000000000000000000000000800b'],
    ['BootloaderUtilities', '0x000000000000000000000000000000000000800c'],
    ['BytecodeCompressor', '0x000000000000000000000000000000000000800e'],
    ['ComplexUpgrader', '0x000000000000000000000000000000000000800f']
]);

// State of the system contracts at a given protocol version and environment.
// Contains information on whether this contract was updated in this protocol release.
type SystemContractState = {
    bytecode: string;
    previousBytecode: string | null;
    wasChanged: boolean;
};

const reset = '\x1b[0m';
const bold = '\x1b[1m';
const red = '\x1b[31m';
const green = '\x1b[32m';

// State of a single chain (environment) at some protocol version.
type ChainState = {
    // Directory where given config is located.
    dir: string;

    // Map from name to system contracts
    systemContracts: Map<string, SystemContractState>;

    bootloader: SystemContractState;
    defaultAA: SystemContractState;
};

// State of the whole ecosystem at a given protocol version.
type ChainsAtProtocolVersion = {
    // mainnet and testnet might NOT be updated to a given version
    // this happens only if we found a serious bug while updating stage (or testnet).
    mainnet2: ChainState | null;
    testnet2: ChainState | null;
    stage2: ChainState;
    upgradeName: string;
    creationTimestamp: string;
    protocolVersion: number;
};

type ProtocolVersion = number;

type ChainStateHistory = Map<ProtocolVersion, ChainsAtProtocolVersion>;

// Type with SystemContracts that is read from the upgrade JSON file.
type SystemContract = {
    name: string;
    bytecodeHashes: string[];
    address: string | null;
};

// Type that is read from the upgrade JSON file - about the ForcedDeployment,
// where we can push any bytecode into any address.
type ForcedDeployment = {
    bytecodeHash: string;
    newAddress: string;
    value: number;
    input: string;
    callConstructor: boolean;
};

function createSystemContractState(
    currentState: SystemContract | null,
    prevState: SystemContractState | null
): SystemContractState {
    if (currentState == null && prevState == null) {
        throw new Error('Both current and prev are null');
    }
    if (currentState == null) {
        let result: SystemContractState = {
            bytecode: prevState!.bytecode,
            previousBytecode: null,
            wasChanged: false
        };
        return result;
    }
    if (currentState.bytecodeHashes.length != 1) {
        throw new Error('Invalid bytecode hash count');
    }

    let currentBytecode = currentState.bytecodeHashes[0];
    let previousBytecode = prevState ? prevState.bytecode : null;

    let result: SystemContractState = {
        bytecode: currentBytecode,
        previousBytecode: previousBytecode,
        wasChanged: currentBytecode != previousBytecode
    };
    return result;
}

function loadChainState(chainStateDir: string, previousChainState: ChainState | null): ChainState | null {
    // Directory might not exist, if we're not updating this environment at this version.
    if (!fs.existsSync(chainStateDir)) {
        return null;
    }

    let l2Upgrade = loadJSON(chainStateDir, 'l2Upgrade.json');

    let defaultAA = previousChainState
        ? createSystemContractState(l2Upgrade['defaultAA'], previousChainState.defaultAA)
        : createSystemContractState(l2Upgrade['defaultAA'], null);
    let bootloader = previousChainState
        ? createSystemContractState(l2Upgrade['bootloader'], previousChainState.defaultAA)
        : createSystemContractState(l2Upgrade['bootloader'], null);

    let sysContracts: SystemContract[] = l2Upgrade['systemContracts'];
    let newEntries: Map<string, SystemContractState> = new Map();
    if (previousChainState) {
        previousChainState.systemContracts.forEach((entry, key) => {
            newEntries.set(key, {
                bytecode: entry.bytecode,
                previousBytecode: null,
                wasChanged: false
            });
        });
    }
    sysContracts.forEach((element) => {
        if (SYSTEM_CONTRACT_MAP.get(element.name) == null) {
            throw new Error(`Invalid system contract name ${element.name}`);
        }
        if (SYSTEM_CONTRACT_MAP.get(element.name) != element.address) {
            throw new Error(`Invalid address for contract ${element.name}`);
        }
        let previous = newEntries.get(element.name);
        if (element.bytecodeHashes.length != 1) {
            throw new Error(`Invalid bytecode hashes count ${element.name}`);
        }

        newEntries.set(element.name, {
            bytecode: element.bytecodeHashes[0],
            previousBytecode: previous ? previous.bytecode : null,
            wasChanged: true
        });
    });

    let result: ChainState = {
        dir: chainStateDir,
        systemContracts: newEntries,
        bootloader: bootloader,
        defaultAA: defaultAA
    };

    return result;
}

function loadChainStateHistoryFromDisk(): ChainStateHistory {
    let result: ChainStateHistory = new Map();
    let lastProtocolVersion = LAST_PROTOCOL_VERSION_BEFORE_UPGRADE_SYSTEM;
    let prevMainnet2: ChainState | null = null;
    let prevTestnet2: ChainState | null = null;
    let prevStage2: ChainState | null = null;

    fs.readdirSync(UPGRADES_DIR).forEach((file) => {
        // Assuming that upgrade directories are sorted in increasing order.
        const upgradePath = path.join(UPGRADES_DIR, file);
        let details = getUpgradeDetails(upgradePath);
        let newProtocolVersion = Number(details['protocolVersion']);
        if (newProtocolVersion != lastProtocolVersion + 1) {
            throw new Error(
                `Protocol version didn't increase when parsing ${file} previous: ${lastProtocolVersion} new: ${newProtocolVersion}`
            );
        }
        lastProtocolVersion = newProtocolVersion;

        let mainnet2 = loadChainState(path.join(upgradePath, 'mainnet2'), prevMainnet2);
        if (mainnet2) {
            prevMainnet2 = mainnet2;
        }
        let testnet2 = loadChainState(path.join(upgradePath, 'testnet2'), prevTestnet2);
        if (testnet2) {
            prevTestnet2 = testnet2;
        }
        // We always expect staging to be updated.
        let stage2 = loadChainState(path.join(upgradePath, 'stage2'), prevStage2)!;
        prevStage2 = stage2;

        let chainsAtProtocolVersion: ChainsAtProtocolVersion = {
            mainnet2: mainnet2,
            testnet2: testnet2,
            stage2: stage2,
            upgradeName: details['name'],
            creationTimestamp: new Date(details['creationTimestamp'] * 1000).toUTCString(),
            protocolVersion: newProtocolVersion
        };

        result.set(newProtocolVersion, chainsAtProtocolVersion);
    });

    return result;
}

// Creates a short summary string, about the changes that happened for a given environment with a
// given protocol version.
// B - means that Bootloader hash was changed, AA that defaultAA hash was changed.
// It also prints the information on how many system contracts were changed.
function getChangeSummary(chainState: ChainState | null): string {
    if (chainState == null) {
        return '---';
    }
    let result = '';
    if (chainState.bootloader.wasChanged) {
        result += 'B, ';
    }
    if (chainState.defaultAA.wasChanged) {
        result += 'AA, ';
    }
    let sysContractChanges = 0;
    chainState.systemContracts.forEach((entry) => {
        if (entry.wasChanged) {
            sysContractChanges += 1;
        }
    });
    if (sysContractChanges > 0) {
        result += `${sysContractChanges} SYS, `;
    }

    return result;
}

function printSummary(history: ChainStateHistory) {
    let upgrades: string[][] = [['Protocol Version', 'Upgrade name', 'Creation Time', 'stage', 'testnet', 'mainnet']];

    history.forEach((chainAtProtocol, protocolVersion) => {
        let row: string[] = [
            protocolVersion.toString(),
            chainAtProtocol.upgradeName,
            chainAtProtocol.creationTimestamp,
            getChangeSummary(chainAtProtocol.stage2),
            getChangeSummary(chainAtProtocol.testnet2),
            getChangeSummary(chainAtProtocol.mainnet2)
        ];
        upgrades.push(row);
    });
    console.log(renderTable(upgrades));
}

function checkBytecodeConsistency(chain1: ChainState, chain2: ChainState) {
    if (chain1.bootloader.bytecode != chain2.bootloader.bytecode) {
        throw new Error(`Inconsistent bootloader bytecode`);
    }
    if (chain1.defaultAA.bytecode != chain2.defaultAA.bytecode) {
        throw new Error(`Inconsistent AA bytecode`);
    }

    for (const key of chain1.systemContracts.keys()) {
        if (!chain2.systemContracts.has(key)) {
            throw new Error(`Missing system contract ${key}`);
        } else {
            const value1 = chain1.systemContracts.get(key)!;
            const value2 = chain2.systemContracts.get(key)!;
            if (value1.bytecode !== value2.bytecode) {
                throw new Error(`Different bytecode for ${key}`);
            }
        }
    }
    for (const key of chain2.systemContracts.keys()) {
        if (!chain1.systemContracts.has(key)) {
            throw new Error(`Missing system contract ${key}`);
        }
    }
}

// Check that the system contracts bytecodes for a given protocol version are the same
// in all the environments.
function checkBytecodeConsistencyBetweenEnvironments(history: ChainStateHistory) {
    console.log('\n== Checking bytecode consistency between environments');
    history.forEach((chainAtProtocol, protocolVersion) => {
        try {
            process.stdout.write(`Protocol ${protocolVersion} :`);
            if (chainAtProtocol.testnet2) {
                process.stdout.write(' testnet2 ');
                checkBytecodeConsistency(chainAtProtocol.testnet2, chainAtProtocol.stage2);
            }
            if (chainAtProtocol.mainnet2) {
                process.stdout.write(' mainnet2 ');
                checkBytecodeConsistency(chainAtProtocol.mainnet2, chainAtProtocol.stage2);
            }
            process.stdout.write(`${green}${bold}[PASS]${reset}\n`);
        } catch (error) {
            if (error instanceof Error) {
                process.stdout.write(`${red}${bold}[FAIL]${reset}: ${error.message}\n`);
            } else {
                throw error;
            }
        }
    });
}

function checkForceDeploymentForChainState(chainState: ChainState) {
    let l2Upgrade = loadJSON(chainState.dir, 'l2Upgrade.json');
    let forceDeploymentMap: Map<string, ForcedDeployment> = new Map();

    let nextForcedDeployments: ForcedDeployment[] = l2Upgrade['forcedDeployments'];
    for (const deployment of nextForcedDeployments) {
        if (forceDeploymentMap.has(deployment.bytecodeHash)) {
            throw new Error(`${red}Bytecode force deployed twice: ${deployment.bytecodeHash}${reset}`);
        }
        if (deployment.value != 0) {
            throw new Error(`${red}Force deployment wrong value for ${deployment.bytecodeHash}${reset}`);
        }
        if (deployment.input != '0x') {
            throw new Error(`${red}Force deployment wrong input for ${deployment.bytecodeHash}${reset}`);
        }
        if (deployment.callConstructor != false) {
            throw new Error(`${red}Force deployment wrong call Constructor for ${deployment.bytecodeHash}${reset}`);
        }

        forceDeploymentMap.set(deployment.bytecodeHash, deployment);
    }

    chainState.systemContracts.forEach((systemContractState, key) => {
        if (systemContractState.wasChanged) {
            if (!forceDeploymentMap.get(systemContractState.bytecode)) {
                throw new Error(`Missing force deployment for bytecode for ${key}`);
            }
            if (forceDeploymentMap.get(systemContractState.bytecode)!.newAddress != SYSTEM_CONTRACT_MAP.get(key)!) {
                throw new Error(`Wrong deployment address`);
            }
            forceDeploymentMap.delete(systemContractState.bytecode);
        }
    });
    if (forceDeploymentMap.size > 0) {
        throw new Error('Force deployment has additional entries');
    }

    if (
        l2Upgrade['forcedDeploymentCalldata'] != l2Upgrade['calldata'] ||
        l2Upgrade['calldata'] != l2Upgrade['tx']['data']
    ) {
        throw new Error(`forcedDeployment calldata and calldata or tx data doesn't match`);
    }
    // Calldata to parse (hexadecimal string)
    const calldata = l2Upgrade['calldata'];

    // Create an instance of defaultAbiCoder
    const contractInterface = new ethers.utils.Interface(forcedDeploymentABI);

    const decodedData = contractInterface.decodeFunctionData('forceDeployOnAddresses', calldata);

    // Compare force deployments

    if (decodedData['_deployments'].length != nextForcedDeployments.length) {
        throw new Error(
            `${red}Calldata deployments size ${nextForcedDeployments.length} doesnt match forced Deployments ${decodedData['_deployments'].length} ${reset}`
        );
    }

    for (let i = 0; i < nextForcedDeployments.length; i++) {
        if (decodedData['_deployments'][i].bytecodeHash != nextForcedDeployments[i].bytecodeHash) {
            throw new Error(`${red}Bytecode hash differs for ${i}${reset}`);
        }
        if (
            decodedData['_deployments'][i].newAddress.toLowerCase() != nextForcedDeployments[i].newAddress.toLowerCase()
        ) {
            throw new Error(
                `${red}Address differs for ${i} - ${decodedData['_deployments'][i].newAddress} vs ${nextForcedDeployments[i].newAddress} ${reset}`
            );
        }
        if (decodedData['_deployments'][i].value != nextForcedDeployments[i].value) {
            throw new Error(`${red}Value differs for ${i}  ${reset}`);
        }
        if (decodedData['_deployments'][i].input != nextForcedDeployments[i].input) {
            throw new Error(`${red}Input differs for ${i}  ${reset}`);
        }
        if (decodedData['_deployments'][i].callConstructor != nextForcedDeployments[i].callConstructor) {
            throw new Error(`${red}Call constructor differs for ${i}  ${reset}`);
        }
    }
}

function checkForceDeployments(history: ChainStateHistory) {
    console.log('\n== Checking force deployments');
    history.forEach((chainAtProtocol, protocolVersion) => {
        try {
            process.stdout.write(`Protocol ${protocolVersion} :`);
            process.stdout.write(' stage2 ');
            checkForceDeploymentForChainState(chainAtProtocol.stage2);
            if (chainAtProtocol.testnet2) {
                process.stdout.write(' testnet2 ');
                checkForceDeploymentForChainState(chainAtProtocol.testnet2);
            }
            if (chainAtProtocol.mainnet2) {
                process.stdout.write(' mainnet2 ');
                checkForceDeploymentForChainState(chainAtProtocol.mainnet2);
            }
            process.stdout.write(`${green}${bold}[PASS]${reset}\n`);
        } catch (error) {
            if (error instanceof Error) {
                process.stdout.write(`${red}${bold}[FAIL]${reset}: ${error.message}\n`);
            } else {
                throw error;
            }
        }
    });
}

function checks(history: ChainStateHistory) {
    console.log('=== Starting checks ====');
    checkBytecodeConsistencyBetweenEnvironments(history);
    checkForceDeployments(history);
}

const forcedDeploymentABI = [
    {
        inputs: [
            {
                components: [
                    {
                        internalType: 'bytes32',
                        name: 'bytecodeHash',
                        type: 'bytes32'
                    },
                    {
                        internalType: 'address',
                        name: 'newAddress',
                        type: 'address'
                    },
                    {
                        internalType: 'bool',
                        name: 'callConstructor',
                        type: 'bool'
                    },
                    {
                        internalType: 'uint256',
                        name: 'value',
                        type: 'uint256'
                    },
                    {
                        internalType: 'bytes',
                        name: 'input',
                        type: 'bytes'
                    }
                ],
                internalType: 'struct ContractDeployer.ForceDeployment[]',
                name: '_deployments',
                type: 'tuple[]'
            }
        ],
        name: 'forceDeployOnAddresses',
        outputs: [],
        stateMutability: 'payable',
        type: 'function'
    }
];

function renderTable(data: string[][]): string {
    // Calculate column widths based on maximum cell width in each column
    const colWidths = Array(data[0].length).fill(0);
    for (let row of data) {
        for (let [i, cell] of row.entries()) {
            colWidths[i] = Math.max(colWidths[i], cell.length);
        }
    }

    // Generate horizontal separator line
    const separator = '+' + colWidths.map((width) => '-'.repeat(width + 2)).join('+') + '+';

    // Render each row
    const rows = data.map((row) => {
        return '|' + row.map((cell, i) => ` ${cell.padEnd(colWidths[i])} `).join('|') + '|';
    });

    // Combine rows with separators
    return separator + '\n' + rows.join('\n' + separator + '\n') + '\n' + separator;
}

function getUpgradeDetails(upgradeDir: string) {
    const p = path.join(upgradeDir, 'common.json');

    const fileContent = fs.readFileSync(p, 'utf-8');

    let contents = JSON.parse(fileContent);
    return contents;
}

function loadJSON(dir: string, file: string) {
    const p = path.join(dir, file);
    const fileContent = fs.readFileSync(p, 'utf-8');
    let contents = JSON.parse(fileContent);
    return contents;
}

export const command = new Command('verify-upgrade')
    .description('Verify system upgrades')
    .action(async (_cmd: Command) => {
        let chainUpgradeHistory = loadChainStateHistoryFromDisk();
        printSummary(chainUpgradeHistory);
        checks(chainUpgradeHistory);
    });
