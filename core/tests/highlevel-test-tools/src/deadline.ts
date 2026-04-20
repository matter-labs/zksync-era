/**
 * Returns true if the error looks like a TCP connection failure (server is dead).
 */
export function isConnectionError(error: unknown): boolean {
    const msg = String(error).toLowerCase();
    return (
        msg.includes('connection refused') ||
        msg.includes('econnrefused') ||
        msg.includes('socket hang up') ||
        msg.includes('econnreset')
    );
}

/**
 * Quick health check: sends a trivial RPC call to verify the server is reachable.
 * Returns true if the server responds, false if connection is refused / unreachable.
 */
export async function checkRpcHealth(rpcUrl: string): Promise<boolean> {
    try {
        const resp = await fetch(rpcUrl, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_blockNumber', params: [] }),
            signal: AbortSignal.timeout(5000)
        });
        return resp.ok;
    } catch {
        return false;
    }
}

export type HealthStatus = 'healthy' | 'failing' | 'dead';

/**
 * Tracks consecutive RPC health-check failures for a single endpoint.
 *
 *   const guard = new RpcHealthGuard(url, 3, 'gateway');
 *   const status = await guard.check();   // 'healthy' | 'failing' | 'dead'
 */
export class RpcHealthGuard {
    private consecutiveFailures = 0;

    constructor(
        private rpcUrl: string,
        private maxFailures: number = 3,
        private label: string = rpcUrl
    ) {}

    async check(): Promise<HealthStatus> {
        const healthy = await checkRpcHealth(this.rpcUrl);
        if (healthy) {
            this.consecutiveFailures = 0;
            return 'healthy';
        }
        this.consecutiveFailures++;
        console.log(`⚠️ Health check failed for ${this.label} (${this.consecutiveFailures}/${this.maxFailures})`);
        return this.consecutiveFailures >= this.maxFailures ? 'dead' : 'failing';
    }
}
