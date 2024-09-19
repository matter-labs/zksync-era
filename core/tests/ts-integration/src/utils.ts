import { exec as _exec, spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'child_process';
import { assert } from 'chai';
import { FileConfig } from 'utils/build/file-configs';
import { Tester } from './tester';
import { killPidWithAllChilds } from 'utils/build/kill';
import * as utils from 'utils';
import fs from 'node:fs/promises';
import * as zksync from 'zksync-ethers';

// executes a command in background and returns a child process handle
// by default pipes data to parent's stdio but this can be overridden
export function background({
    command,
    stdio = 'inherit',
    cwd,
    env
}: {
    command: string;
    stdio: any;
    cwd?: ProcessEnvOptions['cwd'];
    env?: ProcessEnvOptions['env'];
}): ChildProcessWithoutNullStreams {
    command = command.replace(/\n/g, ' ');
    console.log(`Run command ${command}`);
    return _spawn(command, { stdio: stdio, shell: true, detached: true, cwd, env });
}

export function runInBackground({
    command,
    components,
    stdio,
    cwd,
    env
}: {
    command: string;
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    env?: Parameters<typeof background>[0]['env'];
}): ChildProcessWithoutNullStreams {
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    return background({ command, stdio, cwd, env });
}

export function runServerInBackground({
    components,
    stdio,
    cwd,
    env,
    useZkInception,
    newL1GasPrice,
    newPubdataPrice,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    env?: Parameters<typeof background>[0]['env'];
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
        command = `DATABASE_MERKLE_TREE_MODE=full ${command}`;

        if (newPubdataPrice) {
            command = `ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_PUBDATA_PRICE=${newPubdataPrice} ${command}`;
        }

        if (newL1GasPrice) {
            // We need to ensure that each transaction gets into its own batch for more fair comparison.
            command = `ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_L1_GAS_PRICE=${newL1GasPrice}  ${command}`;
        }

        const testMode = newPubdataPrice || newL1GasPrice;
        if (testMode) {
            // We need to ensure that each transaction gets into its own batch for more fair comparison.
            command = `CHAIN_STATE_KEEPER_TRANSACTION_SLOTS=1 ${command}`;
        }
    }
    return runInBackground({ command, components, stdio, cwd, env });
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
    constructor(
        public readonly tester: Tester,
        private readonly proc: ChildProcessWithoutNullStreams,
        private readonly type: TYPE
    ) { }

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

    /** Waits for the node process to exit. */
    public async waitForExit(): Promise<number> {
        while (this.proc.exitCode === null) {
            await utils.sleep(1);
        }
        return this.proc.exitCode;
    }

    public async killAndWaitForShutdown() {
        await this.terminate();
        // Wait until it's really stopped.
        let iter = 0;
        while (iter < 30) {
            try {
                await this.tester.syncWallet.provider.getBlockNumber();
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
        private readonly env?: ProcessEnvOptions['env']
    ) { }

    public async spawnMainNode(newL1GasPrice?: string, newPubdataPrice?: string): Promise<Node<NodeType.MAIN>> {
        const env = this.env ?? process.env;
        const { fileConfig, pathToHome, options, logs } = this;

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
            newL1GasPrice: newL1GasPrice,
            newPubdataPrice: newPubdataPrice,
            chain: fileConfig.chain
        });

        // Wait until the main node starts responding.
        const tester = await Tester.init(
            options.ethClientWeb3Url,
            options.apiWeb3JsonRpcHttpUrl,
            options.baseTokenAddress
        );
        await waitForNodeToStart(tester, proc, options.apiWeb3JsonRpcHttpUrl);
        return new Node(tester, proc, NodeType.MAIN);
    }
}

async function waitForNodeToStart(tester: Tester, proc: ChildProcessWithoutNullStreams, l2Url: string) {
    while (true) {
        try {
            const blockNumber = await tester.syncWallet.provider.getBlockNumber();
            console.log(`Initialized node API on ${l2Url}; latest block: ${blockNumber}`);
            break;
        } catch (err) {
            if (proc.exitCode != null) {
                assert.fail(`server failed to start, exitCode = ${proc.exitCode}`);
            }
            console.log(`Node waiting for API on ${l2Url}`);
            await utils.sleep(1);
        }
    }
}
