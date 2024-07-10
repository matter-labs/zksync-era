import { background } from '.';

// TODO: change to use `zk_inception` once migration is complete
const BASE_COMMAND = 'zk_inception server';
const BASE_COMMAND_WITH_ZK = 'zk server';

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
    let command = useZkInception ? BASE_COMMAND : BASE_COMMAND_WITH_ZK;
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    background({ command, stdio, cwd, env });
}
