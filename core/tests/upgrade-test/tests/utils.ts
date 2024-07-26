import { background } from 'utils';

export function runServerInBackground({
    components,
    stdio,
    cwd,
    useZkInception
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    useZkInception?: boolean;
}) {
    let command = useZkInception
        ? 'zk_inception server'
        : 'cd $ZKSYNC_HOME && cargo run --bin zksync_server --release --';
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    background({ command, stdio, cwd });
}
