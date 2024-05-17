/**
 * Shared utils for recovery tests.
 */

import fetch, { FetchError } from 'node-fetch';
import { promisify } from 'node:util';
import { ChildProcess, exec } from 'node:child_process';

export interface Health<T> {
    readonly status: string;
    readonly details?: T;
}

export interface SnapshotRecoveryDetails {
    readonly snapshot_l1_batch: number;
    readonly snapshot_l2_block: number;
    readonly factory_deps_recovered: boolean;
    readonly tokens_recovered: boolean;
    readonly storage_logs_chunks_left_to_process: number;
}

export interface ConsistencyCheckerDetails {
    readonly first_checked_batch?: number;
    readonly last_checked_batch?: number;
}

export interface ReorgDetectorDetails {
    readonly last_correct_l1_batch?: number;
    readonly last_correct_l2_block?: number;
}

export interface TreeDetails {
    readonly min_l1_batch_number?: number | null;
    readonly next_l1_batch_number?: number;
}

export interface DbPrunerDetails {
    readonly last_soft_pruned_l1_batch?: number;
    readonly last_hard_pruned_l1_batch?: number;
}

export interface TreeDataFetcherDetails {
    readonly last_updated_l1_batch?: number;
}

export interface HealthCheckResponse {
    readonly status: string;
    readonly components: {
        snapshot_recovery?: Health<SnapshotRecoveryDetails>;
        consistency_checker?: Health<ConsistencyCheckerDetails>;
        reorg_detector?: Health<ReorgDetectorDetails>;
        tree?: Health<TreeDetails>;
        db_pruner?: Health<DbPrunerDetails>;
        tree_pruner?: Health<{}>;
        tree_data_fetcher?: Health<TreeDataFetcherDetails>;
    };
}

export async function sleep(millis: number) {
    await new Promise((resolve) => setTimeout(resolve, millis));
}

export async function getExternalNodeHealth() {
    const EXTERNAL_NODE_HEALTH_URL = 'http://127.0.0.1:3081/health';

    try {
        const response: HealthCheckResponse = await fetch(EXTERNAL_NODE_HEALTH_URL).then((response) => response.json());
        return response;
    } catch (e) {
        let displayedError = e;
        if (e instanceof FetchError && e.code === 'ECONNREFUSED') {
            displayedError = '(connection refused)'; // Don't spam logs with "connection refused" messages
        }
        console.log(
            `Request to EN health check server failed: ${displayedError}. In CI, you can see more details ` +
                'in "Show * logs" steps'
        );
        return null;
    }
}

export enum NodeComponents {
    STANDARD = 'all',
    WITH_TREE_FETCHER = 'all,tree_fetcher',
    WITH_TREE_FETCHER_AND_NO_TREE = 'core,api,tree_fetcher'
}

export function externalNodeArgs(components: NodeComponents = NodeComponents.STANDARD) {
    const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
    const args = ['external-node', '--', `--components=${components}`];
    if (enableConsensus) {
        args.push('--enable-consensus');
    }
    return args;
}

export async function stopExternalNode() {
    interface ChildProcessError extends Error {
        readonly code: number | null;
    }

    try {
        await promisify(exec)('killall -q -INT zksync_external_node');
    } catch (err) {
        const typedErr = err as ChildProcessError;
        if (typedErr.code === 1) {
            // No matching processes were found; this is fine.
        } else {
            throw err;
        }
    }
}

export async function killExternalNode() {
    interface ChildProcessError extends Error {
        readonly code: number | null;
    }

    try {
        await promisify(exec)('killall -q -KILL zksync_external_node');
    } catch (err) {
        const typedErr = err as ChildProcessError;
        if (typedErr.code === 1) {
            // No matching processes were found; this is fine.
        } else {
            throw err;
        }
    }
}

export async function waitForProcess(childProcess: ChildProcess) {
    await new Promise((resolve, reject) => {
        childProcess.on('error', (error) => {
            reject(error);
        });
        childProcess.on('exit', (code) => {
            if (code === 0) {
                resolve(undefined);
            } else {
                reject(new Error(`Process exited with non-zero code: ${code}`));
            }
        });
    });
}
