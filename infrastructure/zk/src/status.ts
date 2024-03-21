import { Command } from 'commander';

import { Pool } from 'pg';
import { ethers } from 'ethers';
import { assert } from 'console';

// Postgres connection pool - must be initialized later - as the ENV variables are set later.
let main_pool: Pool | null = null;
let prover_pool: Pool | null = null;

const GETTER_ABI = [
    'function getTotalBatchesCommitted() view returns (uint256)',
    'function getTotalBatchesVerified() view returns (uint256)',
    'function getVerifierParams() view returns (bytes32, bytes32, bytes32)'
];

const VERIFIER_ABI = ['function verificationKeyHash() view returns (bytes32)'];

export async function query(pool: Pool, text: string, params?: any[]): Promise<any> {
    const res = await pool!.query(text, params);
    return res;
}

async function queryAndReturnRows(pool: Pool, text: string, params?: any[]): Promise<any> {
    const result = await query(pool, text, params);
    return result.rows;
}

async function getProofProgress(l1_batch_number: number) {
    const result = await query(prover_pool!, 'select * from prover_jobs_fri where l1_batch_number = $1', [
        l1_batch_number
    ]);

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

    const compression_results = await query(
        prover_pool!,
        'select * from proof_compression_jobs_fri where l1_batch_number = $1',
        [l1_batch_number]
    );

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
        const blocksCommitted = await contract.getTotalBatchesCommitted();
        const blocksVerified = await contract.getTotalBatchesVerified();
        return [Number(blocksCommitted), Number(blocksVerified)];
    } catch (error) {
        console.error(`Error calling L1 contract: ${error}`);
        return [-1, -1];
    }
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

    let protocol_version = await query(
        prover_pool!,
        'select recursion_scheduler_level_vk_hash from prover_fri_protocol_versions'
    );
    if (protocol_version.rowCount != 1) {
        console.log(`${redStart}Got ${protocol_version.rowCount} rows with protocol versions, expected 1${resetColor}`);
        return;
    }
    let dbHash = ethers.utils.hexlify(protocol_version.rows[0].recursion_scheduler_level_vk_hash);

    console.log(`Verification key in database is ${dbHash}`);
    if (dbHash != verificationKeyHash) {
        console.log(
            `${redStart}Verification hash in DB differs from the one in contract.${resetColor} State keeper might not send prove requests.`
        );
    } else {
        console.log(`${greenStart}Verifier hash matches.${resetColor}`);
    }
}

async function compareVerificationParams() {
    // Setup a provider
    let provider = new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL);

    // Create a contract instance (diamond proxy doesn't expose this one)
    let contract = new ethers.Contract(process.env.CONTRACTS_DIAMOND_PROXY_ADDR!, GETTER_ABI, provider);
    let node, leaf, circuits;
    try {
        [node, leaf, circuits] = await contract.getVerifierParams();
        console.log(`Verifier params on contract are ${node}, ${leaf}, ${circuits}`);
    } catch (error) {
        console.error(`Error calling L1 contract: ${error}`);
        return;
    }

    let protocol_version = await query(
        prover_pool!,
        'select recursion_node_level_vk_hash, recursion_leaf_level_vk_hash, recursion_circuits_set_vks_hash from prover_fri_protocol_versions'
    );
    if (protocol_version.rowCount != 1) {
        console.log(`${redStart}Got ${protocol_version.rowCount} rows with protocol versions, expected 1${resetColor}`);
        return;
    }
    let dbNode = ethers.utils.hexlify(protocol_version.rows[0].recursion_node_level_vk_hash);
    let dbLeaf = ethers.utils.hexlify(protocol_version.rows[0].recursion_leaf_level_vk_hash);
    let dbCircuit = ethers.utils.hexlify(protocol_version.rows[0].recursion_circuits_set_vks_hash);

    let fail = false;

    if (dbNode != node) {
        fail = true;
        console.log(
            `${redStart}Verification node in DB differs from the one in contract ${dbNode} vs ${node}.${resetColor}`
        );
    }
    if (dbLeaf != leaf) {
        fail = true;
        console.log(
            `${redStart}Verification leaf in DB differs from the one in contract ${dbLeaf} vs ${leaf}.${resetColor}`
        );
    }
    if (dbCircuit != circuits) {
        fail = true;
        console.log(
            `${redStart}Verification circuits in DB differs from the one in contract ${dbCircuit} vs ${circuits}.${resetColor}`
        );
    }

    if (fail == false) {
        console.log(`${greenStart}Verification params match.${resetColor}`);
    }
}

const redStart = '\x1b[31m';
const greenStart = '\x1b[32m';
const resetColor = '\x1b[0m';

export async function statusProver() {
    console.log('==== FRI Prover Status ====');

    main_pool = new Pool({ connectionString: process.env.DATABASE_URL });
    prover_pool = new Pool({ connectionString: process.env.DATABASE_PROVER_URL });

    // Fetch the first and most recent sealed batch numbers
    const stateKeeperStatus = (
        await queryAndReturnRows(main_pool, 'select min(number), max(number) from l1_batches')
    )[0];

    console.log(`State keeper: First batch: ${stateKeeperStatus['min']}, recent batch: ${stateKeeperStatus['max']}`);
    const [blockCommitted, blockVerified] = await getL1ValidatorStatus();
    console.log(`L1 state: block verified: ${blockVerified}, block committed: ${blockCommitted}`);

    assert(blockCommitted >= 0);
    assert(blockCommitted <= stateKeeperStatus['max']);

    const ethSenderLag = stateKeeperStatus['max'] - blockCommitted;
    if (ethSenderLag > 0) {
        console.log(
            `${redStart}Eth sender is ${ethSenderLag} behind. Last block committed: ${blockCommitted}. Most recent sealed state keeper batch: ${stateKeeperStatus['max']}.${resetColor}`
        );
    }

    await compareVerificationKeys();
    await compareVerificationParams();

    const nextBlockForVerification = blockVerified + 1;

    console.log(`Next block that should be verified is: ${nextBlockForVerification}`);
    console.log(`Checking status of the proofs...`);
    for (
        let i = nextBlockForVerification;
        i <= Math.min(nextBlockForVerification + 5, Number(stateKeeperStatus['max']));
        i += 1
    ) {
        await getProofProgress(i);
    }
}

export const command = new Command('status').description('show status of the local system');

command.command('prover').action(statusProver);
