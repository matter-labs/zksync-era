import { spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { executeCommand, executeBackgroundCommand } from './execute-command';
import * as console from 'node:console';
import { promisify } from 'node:util';
import { exec } from 'node:child_process';

/**
 * Strips ANSI escape sequences from a string
 * @param str - The string to clean
 * @returns The string with ANSI escape sequences removed
 */
function stripAnsiEscapeCodes(str: string): string {
    // Remove ANSI escape sequences (colors, cursor movements, etc.)
    return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
}

/**
 * Server handle interface
 */
export interface ServerHandle {
    chainName: string;
    process?: ChildProcess;
    kill(): Promise<void>;
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
 * Starts the server for a given chain and waits for it to be ready
 * @param chainName - The name of the chain to start the server for
 * @returns Promise that resolves with a server handle when the server is ready
 */
export async function startServer(chainName: string): Promise<ServerHandle> {
    console.log(`🚀 Starting server for chain: ${chainName}`);

    const serverHandle: ServerHandle = {
        chainName,
        kill: async () => {
            if (serverHandle.process?.pid) {
                console.log(`🛑 Killing server process for chain: ${chainName} with pid ${serverHandle.process?.pid}`);
                await killPidWithAllChilds(serverHandle.process.pid, 9);
            } else {
                throw new Error('Server is not running!');
            }
        }
    };

    let extraArgs = [];
    // Extract chain type by removing last 9 characters (UUID suffix)
    const chainType = chainName.slice(0, -9);
    extraArgs.push(
        '--components=api,tree,eth,state_keeper,housekeeper,commitment_generator,da_dispatcher,vm_runner_protective_reads,consensus'
    );

    // Start the server in background using executeBackgroundCommand
    serverHandle.process = await executeBackgroundCommand(
        'zkstack',
        ['server', '--ignore-prerequisites', '--chain', chainName].concat(extraArgs),
        chainName,
        'main_node'
    );

    try {
        console.log(`⏳ Waiting for server to be ready: ${chainName}`);

        // Use executeCommand to wait for the server to be ready
        await executeCommand(
            'zkstack',
            ['server', 'wait', '--ignore-prerequisites', '--verbose', '--chain', chainName],
            chainName,
            'main_node'
        );

        console.log(`✅ Server is ready: ${chainName}`);
    } catch (error) {
        console.error(`❌ Error during server wait: ${error}`);
        throw error;
    }

    return serverHandle;
}
