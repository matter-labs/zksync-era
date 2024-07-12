import { exec as _exec, spawn as _spawn, type ProcessEnvOptions } from 'child_process';
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
}) {
    command = command.replace(/\n/g, ' ');
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
}) {
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    background({ command, stdio, cwd, env });
}

export function runServerInBackground({
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
}) {
    let command = useZkInception ? 'zk_inception server' : 'zk server';
    runInBackground({ command, components, stdio, cwd, env });
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
}) {
    let command = useZkInception ? 'zk_inception external-node run' : 'zk external-node';
    runInBackground({ command, components, stdio, cwd, env });
}

// async executor of shell commands
// spawns a new shell and can execute arbitrary commands, like "ls -la | grep .env"
// returns { stdout, stderr }
const promisified = promisify(_exec);
export function exec(command: string, options: ProcessEnvOptions) {
    command = command.replace(/\n/g, ' ');
    return promisified(command, options);
}
