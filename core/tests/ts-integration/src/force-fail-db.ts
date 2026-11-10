/** Direct core-DB access: force-fail marks are inserted by the operator, there is no RPC for them. */
import { Client } from 'pg';
import { loadConfig } from 'utils/build/file-configs';

/** Core DB URL of the *main node* of `chain` (never the external node's copy). */
export function mainNodeDbUrl(pathToHome: string, chain: string): string {
    const secrets = loadConfig({ pathToHome, chain, config: 'secrets.yaml' });
    const url = secrets?.database?.server_url;
    if (typeof url !== 'string') {
        throw new Error(`database.server_url missing in chains/${chain}/configs/secrets.yaml`);
    }
    return url;
}

/** Same statement as `ForceFailDal::insert_force_failed_priority_op`. */
export async function insertForceFailedPriorityOp(
    dbUrl: string,
    priorityOpId: bigint,
    reason: string,
    createdBy: string
): Promise<void> {
    const client = new Client({ connectionString: dbUrl });
    await client.connect();
    try {
        await client.query(
            'INSERT INTO force_failed_priority_ops (priority_op_id, reason, created_by) VALUES ($1, $2, $3) ON CONFLICT (priority_op_id) DO NOTHING',
            [priorityOpId.toString(), reason, createdBy]
        );
    } finally {
        await client.end();
    }
}
