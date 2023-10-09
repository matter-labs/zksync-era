import {spawn as _spawn} from "child_process";

export function spawn(command: string) {
    command = command.replace(/\n/g, ' ');
    const child = _spawn(command, { stdio: 'inherit', shell: true });
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => {
            code == 0 ? resolve(code) : reject(`Child process exited with code ${code}`);
        });
    });
}

export function updateContractsEnv(deployLog: String, envVars: Array<string>) {
    let updatedContracts = '';
    for (const envVar of envVars) {
        const pattern = new RegExp(`${envVar}=.*`, 'g');
        const matches = deployLog.match(pattern);
        if (matches !== null) {
            const varContents = matches[0];
            // env.modify(envVar, varContents);
            updatedContracts += `${varContents}\n`;
        }
    }

    return updatedContracts;
}
