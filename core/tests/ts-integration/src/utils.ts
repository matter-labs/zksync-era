import { spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'child_process';
import { assert } from 'chai';
import { FileConfig } from 'utils/build/file-configs';
import { killPidWithAllChilds } from 'utils/build/kill';
import * as utils from 'utils';
import fs from 'node:fs/promises';
import * as zksync from 'zksync-ethers';
import {
    deleteInternalEnforcedL1GasPrice,
    deleteInternalEnforcedPubdataPrice,
    setInternalEnforcedL1GasPrice,
    setInternalEnforcedPubdataPrice,
    setTransactionSlots
} from '../tests/utils';

// executes a command in background and returns a child process handle
// by default pipes data to parent's stdio but this can be overridden
export function runServerInBackground({
    components,
    stdio,
    cwd,
    env,
    useZkInception,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: ProcessEnvOptions['cwd'];
    env?: ProcessEnvOptions['env'];
    useZkInception?: boolean;
    newL1GasPrice?: string;
    newPubdataPrice?: string;
    chain?: string;
}): ChildProcessWithoutNullStreams {
    let command = '';
    if (useZkInception) {
        command = 'zk_inception server';
        if (chain) {
            command += ` --chain ${chain}`;
        }
    } else {
        command = 'zk server';
    }
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    command = command.replace(/\n/g, ' ');
    console.log(`Run command ${command}`);
    return _spawn(command, { stdio: stdio, shell: true, detached: true, cwd, env });
}

export interface MainNodeSpawnOptions {
    enableConsensus: boolean;
    ethClientWeb3Url: string;
    apiWeb3JsonRpcHttpUrl: string;
    baseTokenAddress: string;
}

export enum NodeType {
    MAIN = 'zksync_server',
    EXT = 'zksync_external_node'
}

export class Node<TYPE extends NodeType> {
    constructor(public proc: ChildProcessWithoutNullStreams, public l2NodeUrl: string, private readonly type: TYPE) {}

    public async terminate() {
        try {
            await killPidWithAllChilds(this.proc.pid!, 9);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    /**
     * Terminates all main node processes running.
     *
     * WARNING: This is not safe to use when running nodes on multiple chains.
     */
    public static async killAll(type: NodeType) {
        try {
            await utils.exec(`killall -KILL ${type}`);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    public async killAndWaitForShutdown() {
        await this.terminate();
        // Wait until it's really stopped.
        let iter = 0;
        while (iter < 30) {
            try {
                let provider = new zksync.Provider(this.l2NodeUrl);
                await provider.getBlockNumber();
                await utils.sleep(2);
                iter += 1;
            } catch (_) {
                // When exception happens, we assume that server died.
                return;
            }
        }
        // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
        throw new Error(`${this.type} didn't stop after a kill request`);
    }
}

export class NodeSpawner {
    public constructor(
        private readonly pathToHome: string,
        private readonly logs: fs.FileHandle,
        private readonly fileConfig: FileConfig,
        private readonly options: MainNodeSpawnOptions,
        private env?: ProcessEnvOptions['env']
    ) {}

    public async spawnMainNode(newL1GasPrice?: string, newPubdataPrice?: string): Promise<Node<NodeType.MAIN>> {
        const env = this.env ?? process.env;
        const { fileConfig, pathToHome, options, logs } = this;

        const testMode = newPubdataPrice || newL1GasPrice;

        console.log('New L1 Gas Price: ', newL1GasPrice);
        console.log('New Pubdata Price: ', newPubdataPrice);

        if (fileConfig.loadFromFile) {
            setTransactionSlots(pathToHome, fileConfig, testMode ? 1 : 8192);

            if (newL1GasPrice) {
                setInternalEnforcedL1GasPrice(pathToHome, fileConfig, parseFloat(newL1GasPrice));
            } else {
                deleteInternalEnforcedL1GasPrice(pathToHome, fileConfig);
            }

            if (newPubdataPrice) {
                setInternalEnforcedPubdataPrice(pathToHome, fileConfig, parseFloat(newPubdataPrice));
            } else {
                deleteInternalEnforcedPubdataPrice(pathToHome, fileConfig);
            }
        } else {
            env['DATABASE_MERKLE_TREE_MODE'] = 'full';

            if (newPubdataPrice) {
                env['ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_PUBDATA_PRICE'] = newPubdataPrice;
            }

            if (newL1GasPrice) {
                // We need to ensure that each transaction gets into its own batch for more fair comparison.
                env['ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_L1_GAS_PRICE'] = newL1GasPrice;
            }

            if (testMode) {
                // We need to ensure that each transaction gets into its own batch for more fair comparison.
                env['CHAIN_STATE_KEEPER_TRANSACTION_SLOTS'] = '1';
            }
        }

        let components = 'api,tree,eth,state_keeper,da_dispatcher,vm_runner_protective_reads';
        if (options.enableConsensus) {
            components += ',consensus';
        }
        if (options.baseTokenAddress != zksync.utils.LEGACY_ETH_ADDRESS) {
            components += ',base_token_ratio_persister';
        }
        let proc = runServerInBackground({
            components: [components],
            stdio: ['ignore', logs, logs],
            cwd: pathToHome,
            env: env,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Wait until the main node starts responding.
        await waitForNodeToStart(proc, options.apiWeb3JsonRpcHttpUrl);
        return new Node(proc, options.apiWeb3JsonRpcHttpUrl, NodeType.MAIN);
    }
}

async function waitForNodeToStart(proc: ChildProcessWithoutNullStreams, l2Url: string) {
    while (true) {
        try {
            const l2Provider = new zksync.Provider(l2Url);
            const blockNumber = await l2Provider.getBlockNumber();
            if (blockNumber != 0) {
                console.log(`Initialized node API on ${l2Url}; latest block: ${blockNumber}`);
                break;
            }
        } catch (err) {
            if (proc.exitCode != null) {
                assert.fail(`server failed to start, exitCode = ${proc.exitCode}`);
            }
            console.log(`Node waiting for API on ${l2Url}`);
            await utils.sleep(1);
        }
    }
}
