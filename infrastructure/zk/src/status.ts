import { Command } from 'commander';
import * as utils from './utils';
import fs from 'fs';

import { Pool } from 'pg';
import { ethers } from 'ethers';
import { assert } from 'console';

const pool = new Pool({
    user: 'postgres',
    host: 'localhost',
    database: 'zksync_local',
    password: 'postgres',
    port: 5432
});

const GETTER_ABI = [
    {
        constant: true,
        inputs: [],
        name: 'getTotalBlocksCommitted',
        outputs: [
            {
                name: '',
                type: 'uint256'
            }
        ],
        payable: false,
        stateMutability: 'view',
        type: 'function'
    },
    {
        constant: true,
        inputs: [],
        name: 'getTotalBlocksVerified',
        outputs: [
            {
                name: '',
                type: 'uint256'
            }
        ],
        payable: false,
        stateMutability: 'view',
        type: 'function'
    }
];

const VERIFIER_ABI = [
    {
        inputs: [],
        name: 'verificationKeyHash',
        outputs: [
            {
                internalType: 'bytes32',
                name: 'vkHash',
                type: 'bytes32'
            }
        ],
        stateMutability: 'pure',
        type: 'function'
    }
];

export async function query(text: string, params?: any[]): Promise<any> {
    const start = Date.now();
    const res = await pool.query(text, params);
    const duration = Date.now() - start;
    return res;
}

async function queryAndReturnRows(text: string, params?: any[]): Promise<any> {
    const result = await query(text, params);
    return result.rows;
}

async function getProofProgress(l1_batch_number: number) {
    const result = await query('select * from prover_jobs_fri where l1_batch_number = $1 ', [l1_batch_number]);

    let successful = 0;
    let failed = 0;
    let in_progress = 0;
    let queued = 0;

    result.rows.forEach((element: { status: string; error: string | undefined }) => {
        if (element.status == 'successful') {
            successful += 1;
        } else {
            if (element.error != null) {
                failed += 1;
            } else {
                if (element.status == 'queued') {
                    queued += 1;
                }
                if (element.status == 'in_progress') {
                    in_progress += 1;
                }
            }
        }
    });

    const compression_results = await query('select * from proof_compression_jobs_fri where l1_batch_number = $1 ', [
        l1_batch_number
    ]);

    let compression_result_string = '';
    if (compression_results.rowCount == 0) {
        compression_result_string = `${redStart}[No compression job found]${resetColor}`;
    } else {
        if (compression_results.rowCount > 1) {
            compression_result_string = `${redStart}[${compression_results.rowCount} compression jobs found - expected just 1]${resetColor}`;
        } else {
            compression_result_string = `Compression job status: ${compression_results.rows[0].status}`;
        }
    }

    console.log(
        `Proof progress for ${l1_batch_number} : ${successful} successful, ${failed} failed, ${in_progress} in progress, ${queued} queued.  ${compression_result_string}`
    );
}

async function getL1ValidatorStatus(): Promise<[number, number]> {
    // Setup a provider
    let provider = new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL);

    // Create a contract instance
    let contract = new ethers.Contract(process.env.CONTRACTS_DIAMOND_PROXY_ADDR!, GETTER_ABI, provider);

    try {
        const blocksCommitted = await contract.getTotalBlocksCommitted();
        const blocksVerified = await contract.getTotalBlocksVerified();
        return [Number(blocksCommitted), Number(blocksVerified)];
    } catch (error) {
        console.error(`Error calling L1 contract: ${error}`);
        return [-1, -1];
    }
}
function stringToHex(str: string) {
    var hex = '';
    for (var i = 0; i < str.length; i++) {
        hex += ('0' + str.charCodeAt(i).toString(16)).slice(-2);
    }
    return hex;
}
function bytesToHex(bytes: Uint8Array): string {
    return bytes.reduce((hex, byte) => hex + byte.toString(16).padStart(2, '0'), '');
}

async function compareVerificationKeys() {
    // Setup a provider
    let provider = new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL);

    // Create a contract instance (diamond proxy doesn't expose this one)
    let contract = new ethers.Contract(process.env.CONTRACTS_VERIFIER_ADDR!, VERIFIER_ABI, provider);
    let verificationKeyHash;
    try {
        verificationKeyHash = await contract.verificationKeyHash();
        console.log(`Verification key hash on contract is ${verificationKeyHash}`);
    } catch (error) {
        console.error(`Error calling L1 contract: ${error}`);
        return;
    }

    let protocol_version = await query('select recursion_scheduler_level_vk_hash from prover_protocol_versions');
    if (protocol_version.rowCount != 1) {
        console.log(`${redStart}Got ${protocol_version.rowCount} rows with protocol versions, expected 1${resetColor}`);
        return;
    }
    let dbHash = '0x' + bytesToHex(protocol_version.rows[0].recursion_scheduler_level_vk_hash);


    console.log(`Verification key in database is ${dbHash}`);
    if (dbHash != verificationKeyHash) {
        console.log(`${redStart}Verification hash in DB differs from the one in contract.${resetColor} State keeper might not send prove requests.}`)
    }

}

const redStart = '\x1b[31m';
const resetColor = '\x1b[0m';

export async function statusProver() {
    console.log('==== FRI Prover status ====');
    if (process.env.ETH_SENDER_SENDER_PROOF_LOADING_MODE != 'FriProofFromGcs') {
        console.log(`${redStart}Can only show status for FRI provers.${resetColor}`);
        return;
    }
    const stateKeeperStatus = (await queryAndReturnRows('select min(number), max(number) from l1_batches'))[0];

    console.log(`State keeper: First batch: ${stateKeeperStatus['min']}, recent batch: ${stateKeeperStatus['max']}`);
    const [blockCommited, blockVerified] = await getL1ValidatorStatus();
    console.log(`L1 state: block verified: ${blockVerified}, block committed: ${blockCommited}`);

    assert(blockCommited >= 0);
    assert(blockCommited <= stateKeeperStatus['max']);

    if (blockCommited < stateKeeperStatus['max']) {
        console.log(
            `${redStart}Eth sender is behind - block commited ${blockCommited} is smaller than most recent state keeper batch ${stateKeeperStatus['max']}.${resetColor}`
        );
        return;
    }
    await compareVerificationKeys();

    const nextBlockForVerification = blockVerified + 1;

    console.log(`Next block that should be verified is: ${nextBlockForVerification}`);
    console.log(`Checking status of the proofs...`);
    for (
        let i = nextBlockForVerification;
        i <= Math.min(nextBlockForVerification + 5, Number(stateKeeperStatus['max']));
        i += 1
    ) {
        getProofProgress(i);
    }
}

export const command = new Command('status').description('show status of the local system');

command.command('prover').action(statusProver);