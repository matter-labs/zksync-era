import { promisify } from 'node:util';
import { exec } from 'node:child_process';

export async function killPidWithAllChilds(pid: number, signalNumber: number) {
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
        } catch (e) {
            // Killing children first often causes the parent to exit on its own (e.g. SIGCHLD
            // cascades), so the parent is already gone by the time we get to it. That manifests
            // as `kill: No such process` and is expected — only surface unexpected errors.
            const msg = e instanceof Error ? e.message : String(e);
            if (!/No such process/.test(msg)) {
                console.log(`Failed to kill ${childs[i]} with ${e}`);
            }
        }
    }
}
