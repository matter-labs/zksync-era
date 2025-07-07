import { executeCommand, executeBackgroundCommand, removeErrorListeners } from './execute-command';
import { FileMutex } from './file-mutex';
import * as console from 'node:console';
import { ChildProcess } from 'child_process';
import { promisify } from 'node:util';
import { exec } from 'node:child_process';
import { getRpcUrl } from './rpc-utils';
import { TestChain } from './create-chain';

/**
 * Global mutex for chain initialization (same as in create-chain.ts)
 */
const fileMutex = new FileMutex();

/**
 * External node handle class
 */
export class TestExternalNode {
    private expectedErrorCode?: number;

    constructor(
        public readonly chainName: string,
        public process?: ChildProcess,
        public killed: boolean = false,
        private waitForExitPromise: Promise<void> | undefined = undefined
    ) {}

    markAsExpectingErrorCode(errorCode: number): void {
        this.expectedErrorCode = errorCode;
        console.log(`📝 Marked external node ${this.chainName} as expecting error code: ${errorCode}`);
        removeErrorListeners(this.process);
        if (!this.process) {
            throw new Error('External node process is not available!');
        }

        this.waitForExitPromise = new Promise((resolve, reject) => {
            this.process!.on('exit', (code: number | null, signal: string | null) => {
                this.killed = true;
                if (code !== null) {
                    console.log(`🔄 External node ${this.chainName} exited with code: ${code}, signal: ${signal}`);

                    if (this.expectedErrorCode !== undefined) {
                        if (code === this.expectedErrorCode) {
                            console.log(`✅ External node ${this.chainName} exited with expected error code: ${code}`);
                            resolve();
                        } else {
                            const error = new Error(
                                `External node ${this.chainName} exited with unexpected code: ${code}, expected: ${this.expectedErrorCode}`
                            );
                            console.error(`❌ ${error.message}`);
                            reject(error);
                        }
                    } else {
                        resolve();
                    }
                } else if (signal) {
                    console.log(`🔄 External node ${this.chainName} exited with signal: ${signal}`);
                    resolve(); // Treat signal termination as success
                } else {
                    resolve();
                }
            });

            this.process!.on('error', (_) => {
                resolve();
            });
        });
    }

    async waitForExitWithErrorCode(): Promise<void> {
        if (this.waitForExitPromise === undefined) {
            throw new Error('Please call markAsExpectingErrorCode() before using this method');
        }
        return this.waitForExitPromise;
    }

    async kill(): Promise<void> {
        if (this.process?.pid) {
            removeErrorListeners(this.process);
            console.log(`🛑 Killing external node process for chain: ${this.chainName} with pid ${this.process?.pid}`);
            await killPidWithAllChilds(this.process.pid, 9);
        } else {
            throw new Error('External node is not running!');
        }
        this.killed = true;
    }
}

async function killPidWithAllChilds(pid: number, signalNumber: number) {
    let childs = [pid];
    while (true) {
        try {
            let child = childs.at(-1);
            childs.push(+(await promisify(exec)(`pgrep -P ${child}`)).stdout);
        } catch (e) {
            break;
        }
    }
    // We always run the test using additional tools, that means we have to kill not the main process, but the child process
    for (let i = childs.length - 1; i >= 0; i--) {
        try {
            await promisify(exec)(`kill -${signalNumber} ${childs[i]}`);
        } catch (e) {}
    }
}

/**
 * Initializes an external node with the specified configuration
 * @param gatewayRpcUrl - Optional gateway RPC URL. If provided, will be used as --gateway-rpc-url parameter
 * @param chainName - The name of the chain (defaults to 'era')
 * @returns Promise that resolves when the external node is initialized
 */
export async function initExternalNode(chainName: string = 'era'): Promise<void> {
    console.log(`🚀 Initializing external node for chain: ${chainName}`);

    try {
        // Acquire mutex for external node initialization
        console.log(`🔒 Acquiring mutex for external node initialization of ${chainName}...`);
        await fileMutex.acquire();
        console.log(`✅ Mutex acquired for external node initialization of ${chainName}`);

        try {
            // Build the configs command arguments
            const configsArgs = [
                'external-node',
                'configs',
                '--db-url=postgres://postgres:notsecurepassword@localhost:5432',
                `--db-name=${chainName}_external_node`,
                '--l1-rpc-url=http://localhost:8545',
                '--chain',
                chainName,
                '--tight-ports'
            ];

            const useGatewayChain = process.env.USE_GATEWAY_CHAIN;
            if (useGatewayChain === 'WITH_GATEWAY') {
                configsArgs.push(`--gateway-rpc-url=${getRpcUrl('gateway')}`);
            }

            console.log(`⏳ Configuring external node: ${chainName}`);

            // Execute the configs command
            await executeCommand('zkstack', configsArgs, chainName, 'external_node');

            console.log(`✅ External node configured successfully: ${chainName}`);

            console.log(`⏳ Initializing external node: ${chainName}`);

            // Execute the init command
            await executeCommand(
                'zkstack',
                ['external-node', 'init', '--ignore-prerequisites', '--chain', chainName],
                chainName,
                'external_node'
            );

            console.log(`✅ External node initialized successfully: ${chainName}`);
        } finally {
            fileMutex.release();
        }
    } catch (error) {
        console.error(`❌ Error during external node initialization: ${error}`);
        throw error;
    }
}

export async function runExternalNode(testChain: TestChain, chainName: string): Promise<TestExternalNode> {
    // Extract chain type by removing last 9 characters (UUID suffix)
    const chainType = chainName.slice(0, -9);

    console.log(`🚀 Running external node for chain: ${chainName} (type: ${chainType})`);

    try {
        // Step 1: Run external node with chain-specific arguments
        console.log(`⏳ Starting external node: ${chainName}`);

        const runArgs = ['external-node', 'run', '--ignore-prerequisites', '--chain', chainName];

        // Add chain-specific arguments
        switch (chainType) {
            case 'validium':
                runArgs.push('--components', 'all,da_fetcher');
                break;
            case 'da_migration':
                runArgs.push('--components', 'all,da_fetcher');
                break;
            case 'custom_token':
                // No additional arguments needed
                break;
            case 'era':
                // No additional arguments needed for era
                break;
            default:
                console.warn(`⚠️ Unknown chain type: ${chainType}, using default arguments`);
        }

        // Run the external node in background
        const process = await executeBackgroundCommand('zkstack', runArgs, chainName, 'external_node');

        console.log(`✅ External node started successfully: ${chainName}`);

        // Step 2: Wait for external node to be ready
        console.log(`⏳ Waiting for external node to be ready: ${chainName}`);
        await executeCommand(
            'zkstack',
            ['external-node', 'wait', '--ignore-prerequisites', '--verbose', '--chain', chainName],
            chainName,
            'external_node'
        );

        console.log(`✅ External node is ready: ${chainName}`);

        //safeguard in case this method is called not from within TestChain
        testChain.externalNode = new TestExternalNode(chainName, process);
        return testChain.externalNode;
    } catch (error) {
        console.error(`❌ Error running external node: ${error}`);
        throw error;
    }
}
