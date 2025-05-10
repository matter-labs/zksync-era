import { spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'child_process';

// executes a command in background and returns a child process handle
// by default pipes data to parent's stdio but this can be overridden
export function background({
    command,
    stdio = 'inherit',
    cwd
}: {
    command: string;
    stdio: any;
    cwd?: ProcessEnvOptions['cwd'];
}): ChildProcessWithoutNullStreams {
    command = command.replace(/\n/g, ' ');
    console.log(`Running command in background: ${command}`);
    return _spawn(command, { stdio: stdio, shell: true, detached: true, cwd });
}

export function runInBackground({
    command,
    components,
    stdio,
    cwd
}: {
    command: string;
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
}): ChildProcessWithoutNullStreams {
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }

    return background({
        command,
        stdio,
        cwd
    });
}

export function runExternalNodeInBackground({
    components,
    stdio,
    cwd,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    chain: string;
}): ChildProcessWithoutNullStreams {
    const command = `zkstack external-node run --chain ${chain}`;
    return runInBackground({ command, components, stdio, cwd });
}
