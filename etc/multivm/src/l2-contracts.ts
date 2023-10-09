import { spawn, updateContractsEnv } from './utils';
import fs from 'fs';

export async function deployL2(
    version: ProtocolVersions,
    args: any[] = [],
    includePaymaster?: boolean,
    includeWETH?: boolean
) {
    const baseCommandL2 = `yarn --cwd ${l2ContractsPath(version)}`;
    const baseCommandL1 = `yarn --cwd ${l1ContractsPath(version)}`;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    await spawn(`${baseCommandL2} build`);

    await spawn(`${baseCommandL1} initialize-bridges ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await spawn(`${baseCommandL2} deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    if (includeWETH) {
        await spawn(`${baseCommandL2} deploy-l2-weth ${args.join(' ')} | tee -a deployL2.log`);
    }

    await spawn(`${baseCommandL2} deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    const l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(l2DeployLog, l2DeploymentEnvVars);

    if (includeWETH) {
        await spawn(`${baseCommandL1} initialize-weth-bridges ${args.join(' ')} | tee -a deployL1.log`);
    }

    const l1DeployLog = fs.readFileSync('deployL1.log').toString();
    const l1DeploymentEnvVars = ['CONTRACTS_L2_WETH_BRIDGE_ADDR'];
    updateContractsEnv(l1DeployLog, l1DeploymentEnvVars);
}
