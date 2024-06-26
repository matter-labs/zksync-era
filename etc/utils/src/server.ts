import { background } from '.';

// TODO: change to use `zk_inception` once migration is complete
const BASE_COMMAND = 'zk server';

export function runServerInBackground({
    components,
    stdio,
    cwd
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
}) {
    let command = BASE_COMMAND;
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    background({ command, stdio, cwd });
}
