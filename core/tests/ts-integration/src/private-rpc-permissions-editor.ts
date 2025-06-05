// perms-editor.ts
import { promises as fs } from 'node:fs';
import yaml from 'js-yaml';
import { Mutex } from 'async-mutex';

type YamlMethod = {
    signature: string;
    read: { type: 'public' | 'group' | 'checkArgument'; groups?: string[]; argIndex?: number };
    write: { type: 'public' | 'group' | 'checkArgument'; groups?: string[]; argIndex?: number };
};

type YamlContract = { address: string; methods: YamlMethod[] };
type YamlRoot = { groups: any[]; contracts: YamlContract[] };
const mutex = new Mutex();
/**
 * Adds a method with public read/write permissions to the YAML file.
 * Creates the contract entry if it isn't present.
 * Throws on invalid input or I/O failure.
 *
 * @param filePath         – absolute or relative path to the YAML file
 * @param contractAddress  – checksummed or lower-case address
 * @param methodSignature  – Solidity-style signature e.g. `"function foo() (uint256)"`
 */
export async function injectPermissionsToFile(
    filePath: string,
    contractAddress: string,
    methodSignature: string
): Promise<void> {
    await mutex.acquire();
    // --- load & parse ---------------------------------------------------------
    const raw = await fs.readFile(filePath, 'utf8');
    const data = yaml.load(raw) as YamlRoot;

    if (!data || !Array.isArray(data.contracts)) throw new Error('YAML root missing ‘contracts’ array');

    // --- normalise address for comparison ------------------------------------
    const match = (addr: string) => addr.toLowerCase() === contractAddress.toLowerCase();

    // --- ensure contract exists ----------------------------------------------
    let contract = data.contracts.find((c) => match(c.address));
    if (!contract) {
        contract = { address: contractAddress, methods: [] };
        data.contracts.push(contract);
    }

    // --- ensure methods array exists -----------------------------------------
    if (!Array.isArray(contract.methods)) contract.methods = [];

    // --- prevent duplicates ---------------------------------------------------
    const already = contract.methods.some((m) => m.signature === methodSignature);
    if (!already) {
        const newMethod: YamlMethod = {
            signature: methodSignature,
            read: { type: 'public' },
            write: { type: 'public' }
        };
        contract.methods.push(newMethod);
    }

    // --- dump & save ----------------------------------------------------------
    const dumped = yaml.dump(data, { lineWidth: 120, sortKeys: false, quotingType: '"' });
    await fs.writeFile(filePath, dumped, 'utf8');
    mutex.release();
}
