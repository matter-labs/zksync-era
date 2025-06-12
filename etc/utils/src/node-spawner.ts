import { killPidWithAllChilds } from './kill';
import { spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'child_process';
import { exec, sleep } from './index';
import fs from 'node:fs/promises';
import * as zksync from 'zksync-ethers';
import * as fsSync from 'fs';
import YAML from 'yaml';
import { FileConfig, getConfigPath } from './file-configs';

// executes a command in background and returns a child process handle
// by default pipes data to parent's stdio but this can be overridden
export function runServerInBackground({
    components,
    stdio,
    cwd,
    env,
    useZkStack,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: ProcessEnvOptions['cwd'];
    env?: ProcessEnvOptions['env'];
    useZkStack?: boolean;
    chain?: string;
}): ChildProcessWithoutNullStreams {
    let command = '';
    if (useZkStack) {
        command = 'zkstack server';
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
    constructor(
        public proc: ChildProcessWithoutNullStreams,
        public l2NodeUrl: string,
        private readonly type: TYPE
    ) {}

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
            await exec(`killall -KILL ${type}`);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    public async waitForShutdown() {
        // Wait until it's really stopped.
        let iter = 0;
        while (iter < 30) {
            try {
                let provider = new zksync.Provider(this.l2NodeUrl);
                await provider.getBlockNumber();
                await sleep(2);
                iter += 1;
            } catch (_) {
                // When exception happens, we assume that server died.
                return;
            }
        }
        // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
        throw new Error(`${this.type} didn't stop after a kill request`);
    }

    public async killAndWaitForShutdown() {
        await this.terminate();
        // Wait until it's really stopped.
        await this.waitForShutdown();
    }
}

interface MainNodeOptions {
    newL1GasPrice?: bigint;
    newPubdataPrice?: bigint;
    customBaseToken?: boolean;
    externalPriceApiClientForcedNumerator?: number;
    externalPriceApiClientForcedDenominator?: number;
    externalPriceApiClientForcedFluctuation?: number;
    baseTokenPricePollingIntervalMs?: number;
    baseTokenAdjusterL1UpdateDeviationPercentage?: number;
}

export class NodeSpawner {
    private readonly generalConfigPath: string | undefined;
    private readonly originalConfig: string | undefined;
    public mainNode: Node<NodeType.MAIN> | null;

    public constructor(
        private readonly pathToHome: string,
        private readonly logs: fs.FileHandle,
        private readonly fileConfig: FileConfig,
        private readonly options: MainNodeSpawnOptions,
        private env?: ProcessEnvOptions['env']
    ) {
        this.mainNode = null;
        if (fileConfig.loadFromFile) {
            this.generalConfigPath = getConfigPath({
                pathToHome,
                chain: fileConfig.chain,
                configsFolder: 'configs',
                config: 'general.yaml'
            });
            this.originalConfig = fsSync.readFileSync(this.generalConfigPath, 'utf8');
        }
    }

    public async killAndSpawnMainNode(configOverrides: MainNodeOptions | null = null): Promise<void> {
        if (this.mainNode != null) {
            await this.mainNode.killAndWaitForShutdown();
            this.mainNode = null;
        }
        this.mainNode = await this.spawnMainNode(configOverrides);
    }

    private async spawnMainNode(overrides: MainNodeOptions | null): Promise<Node<NodeType.MAIN>> {
        const env = this.env ?? process.env;
        const { fileConfig, pathToHome, options, logs } = this;

        const testMode = overrides?.newPubdataPrice != null || overrides?.newL1GasPrice != null;

        console.log('Overrides: ', overrides);

        if (fileConfig.loadFromFile) {
            this.restoreConfig();
            const config = this.readFileConfig();
            config['state_keeper']['transaction_slots'] = testMode ? 1 : 8192;

            if (overrides != null) {
                if (overrides.newL1GasPrice) {
                    config['eth']['gas_adjuster']['internal_enforced_l1_gas_price'] = overrides.newL1GasPrice;
                }

                if (overrides.newPubdataPrice) {
                    config['eth']['gas_adjuster']['internal_enforced_pubdata_price'] = overrides.newPubdataPrice;
                }

                if (overrides.externalPriceApiClientForcedNumerator !== undefined) {
                    config['external_price_api_client']['forced_numerator'] =
                        overrides.externalPriceApiClientForcedNumerator;
                }

                if (overrides.externalPriceApiClientForcedDenominator !== undefined) {
                    config['external_price_api_client']['forced_denominator'] =
                        overrides.externalPriceApiClientForcedDenominator;
                }

                if (overrides.externalPriceApiClientForcedFluctuation !== undefined) {
                    config['external_price_api_client']['forced_fluctuation'] =
                        overrides.externalPriceApiClientForcedFluctuation;
                }

                if (overrides.baseTokenPricePollingIntervalMs !== undefined) {
                    const cacheUpdateInterval = overrides.baseTokenPricePollingIntervalMs / 2;
                    // To reduce price polling interval we also need to reduce base token receipt checking and tx sending sleeps as they are blocking the poller. Also cache update needs to be reduced appropriately.

                    config['base_token_adjuster']['l1_receipt_checking_sleep_ms'] =
                        overrides.baseTokenPricePollingIntervalMs;
                    config['base_token_adjuster']['l1_tx_sending_sleep_ms'] = overrides.baseTokenPricePollingIntervalMs;
                    config['base_token_adjuster']['price_polling_interval_ms'] =
                        overrides.baseTokenPricePollingIntervalMs;
                    config['base_token_adjuster']['price_cache_update_interval_ms'] = cacheUpdateInterval;
                }

                if (overrides.baseTokenAdjusterL1UpdateDeviationPercentage !== undefined) {
                    config['base_token_adjuster']['l1_update_deviation_percentage'] =
                        overrides.baseTokenAdjusterL1UpdateDeviationPercentage;
                }
            }

            this.writeFileConfig(config);
        } else {
            env['DATABASE_MERKLE_TREE_MODE'] = 'full';

            if (overrides != null) {
                if (overrides.newPubdataPrice) {
                    env['ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_PUBDATA_PRICE'] =
                        overrides.newPubdataPrice.toString();
                }

                if (overrides.newL1GasPrice) {
                    // We need to ensure that each transaction gets into its own batch for more fair comparison.
                    env['ETH_SENDER_GAS_ADJUSTER_INTERNAL_ENFORCED_L1_GAS_PRICE'] = overrides.newL1GasPrice.toString();
                }

                if (overrides.externalPriceApiClientForcedNumerator !== undefined) {
                    env['EXTERNAL_PRICE_API_CLIENT_FORCED_NUMERATOR'] =
                        overrides.externalPriceApiClientForcedNumerator.toString();
                }

                if (overrides.externalPriceApiClientForcedDenominator !== undefined) {
                    env['EXTERNAL_PRICE_API_CLIENT_FORCED_DENOMINATOR'] =
                        overrides.externalPriceApiClientForcedDenominator.toString();
                }

                if (overrides.externalPriceApiClientForcedFluctuation !== undefined) {
                    env['EXTERNAL_PRICE_API_CLIENT_FORCED_FLUCTUATION'] =
                        overrides.externalPriceApiClientForcedFluctuation.toString();
                }

                if (overrides.baseTokenPricePollingIntervalMs !== undefined) {
                    const cacheUpdateInterval = overrides.baseTokenPricePollingIntervalMs / 2;
                    // To reduce price polling interval we also need to reduce base token receipt checking and tx sending sleeps as they are blocking the poller. Also cache update needs to be reduced appropriately.
                    env['BASE_TOKEN_ADJUSTER_L1_RECEIPT_CHECKING_SLEEP_MS'] =
                        overrides.baseTokenPricePollingIntervalMs.toString();
                    env['BASE_TOKEN_ADJUSTER_L1_TX_SENDING_SLEEP_MS'] =
                        overrides.baseTokenPricePollingIntervalMs.toString();
                    env['BASE_TOKEN_ADJUSTER_PRICE_POLLING_INTERVAL_MS'] =
                        overrides.baseTokenPricePollingIntervalMs.toString();
                    env['BASE_TOKEN_ADJUSTER_PRICE_CACHE_UPDATE_INTERVAL_MS'] = cacheUpdateInterval.toString();
                }

                if (overrides.baseTokenAdjusterL1UpdateDeviationPercentage !== undefined) {
                    env['BASE_TOKEN_ADJUSTER_L1_UPDATE_DEVIATION_PERCENTAGE'] =
                        overrides.baseTokenAdjusterL1UpdateDeviationPercentage.toString();
                }
            }

            if (testMode) {
                // We need to ensure that each transaction gets into its own batch for more fair comparison.
                env['CHAIN_STATE_KEEPER_TRANSACTION_SLOTS'] = '1';
            }
        }

        let components = 'api,tree,eth,state_keeper,da_dispatcher,commitment_generator,vm_runner_protective_reads';
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
            useZkStack: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Wait until the main node starts responding.
        await waitForNodeToStart(proc, options.apiWeb3JsonRpcHttpUrl);
        return new Node(proc, options.apiWeb3JsonRpcHttpUrl, NodeType.MAIN);
    }

    public restoreConfig() {
        if (this.generalConfigPath != void 0 && this.originalConfig != void 0)
            fsSync.writeFileSync(this.generalConfigPath, this.originalConfig, 'utf8');
    }

    private readFileConfig() {
        if (this.generalConfigPath == void 0)
            throw new Error('Trying to set property in config while not in file mode');
        const generalConfig = fsSync.readFileSync(this.generalConfigPath, 'utf8');
        return YAML.parse(generalConfig);
    }

    private writeFileConfig(config: any) {
        if (this.generalConfigPath == void 0)
            throw new Error('Trying to set property in config while not in file mode');

        const newGeneralConfig = YAML.stringify(config);
        fsSync.writeFileSync(this.generalConfigPath, newGeneralConfig, 'utf8');
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
                throw new Error(`server failed to start, exitCode = ${proc.exitCode}`);
            }
            console.log(`Node waiting for API on ${l2Url}`);
            await sleep(1);
        }
    }
}
