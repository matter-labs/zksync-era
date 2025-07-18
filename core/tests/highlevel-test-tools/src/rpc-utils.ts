import * as fs from 'fs';
import * as yaml from 'js-yaml';
import * as path from 'path';
import { chainsPath } from './zksync-home';

export function getRpcUrl(chainName: string): string {
    const configPath = path.join(chainsPath(), chainName, 'configs', 'general.yaml');

    if (!fs.existsSync(configPath)) {
        throw new Error(`Config file not found: ${configPath}`);
    }

    const configContent = fs.readFileSync(configPath, 'utf8');
    const config = yaml.load(configContent) as any;

    if (!config.api?.web3_json_rpc?.http_url) {
        throw new Error(`RPC URL not found in config: ${configPath}`);
    }

    return config.api.web3_json_rpc.http_url;
}

export async function queryJsonRpc(rpcUrl: string, method: string, params: any[] = []): Promise<any> {
    const response = await fetch(rpcUrl, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify({
            jsonrpc: '2.0',
            id: 1,
            method,
            params
        })
    });

    if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
    }

    const data = (await response.json()) as any;

    if (data.error) {
        throw new Error(`JSON-RPC error: ${data.error.message}`);
    }

    return data.result;
}

export async function getL1BatchNumber(chainName: string): Promise<number> {
    const rpcUrl = getRpcUrl(chainName);
    console.log(`🔗 Querying L1 batch number from RPC URL: ${rpcUrl}`);

    try {
        const l1BatchNumberHex = await queryJsonRpc(rpcUrl, 'zks_L1BatchNumber');
        const l1BatchNumber = parseInt(l1BatchNumberHex, 16);
        console.log(`📋 L1 batch number from RPC (hex: ${l1BatchNumberHex}, decimal: ${l1BatchNumber})`);
        return l1BatchNumber;
    } catch (error) {
        console.error(`❌ Failed to query L1 batch number: ${error}`);
        throw error;
    }
}

export async function getL1BatchDetails(chainName: string, batchNumber: number): Promise<any> {
    const rpcUrl = getRpcUrl(chainName);
    console.log(`🔗 Querying L1 batch details from RPC URL: ${rpcUrl} for batch ${batchNumber}`);

    try {
        return await queryJsonRpc(rpcUrl, 'zks_getL1BatchDetails', [batchNumber]);
    } catch (error) {
        console.error(`❌ Failed to query L1 batch details: ${error}`);
        throw error;
    }
}
