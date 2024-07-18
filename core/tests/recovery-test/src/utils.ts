import { exec as _exec, spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'child_process';
import { promisify } from 'util';

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
    console.log(`Running command in background: ${command}`);
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

    return background({
        command,
        stdio,
        cwd,
        env
    });
}

export function runExternalNodeInBackground({
    components,
    stdio,
    cwd,
    env,
    useZkInception
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    env?: Parameters<typeof background>[0]['env'];
    useZkInception?: boolean;
}): ChildProcessWithoutNullStreams {
    let command = '';
    if (useZkInception) {
        command = 'zk_inception external-node run';
    } else {
        command = 'zk external-node';

        const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
        if (enableConsensus) {
            command += ' --enable-consensus';
        }
    }
    return runInBackground({ command, components, stdio, cwd, env });
}
