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

/**
 * Polls `fn` until it returns a non-null/non-undefined value, or the deadline expires.
 *
 * @param fn - Async function that returns `T` when the condition is met, or `null`/`undefined` to keep waiting.
 *             If `fn` throws, the error is logged and treated as "not ready yet" (retried).
 *             However, connection errors (ECONNREFUSED etc.) are treated as fatal after 3 consecutive occurrences.
 * @param opts.timeoutMs - Maximum wall-clock time to wait before throwing.
 * @param opts.intervalMs - Sleep between polls (default 2000).
 * @param opts.label - Human-readable context included in the timeout error.
 * @param opts.failOnConnectionError - If true (default), connection errors abort after 3 consecutive occurrences.
 * @returns The first non-null/non-undefined value returned by `fn`.
 */
export async function withDeadline<T>(
    fn: () => Promise<T | null | undefined>,
    opts: { timeoutMs: number; intervalMs?: number; label: string; failOnConnectionError?: boolean }
): Promise<T> {
    const { timeoutMs, intervalMs = 2000, label, failOnConnectionError = true } = opts;
    const start = Date.now();
    let consecutiveConnectionErrors = 0;

    while (true) {
        const elapsed = Date.now() - start;
        if (elapsed >= timeoutMs) {
            throw new Error(
                `[withDeadline] ${label}: timed out after ${(elapsed / 1000).toFixed(1)}s (limit: ${(timeoutMs / 1000).toFixed(0)}s)`
            );
        }

        try {
            const result = await fn();
            if (result !== null && result !== undefined) {
                return result;
            }
            consecutiveConnectionErrors = 0;
        } catch (error) {
            if (failOnConnectionError && isConnectionError(error)) {
                consecutiveConnectionErrors++;
                if (consecutiveConnectionErrors >= 3) {
                    throw new Error(
                        `[withDeadline] ${label}: server unreachable (${consecutiveConnectionErrors} consecutive connection errors). Last error: ${error}`
                    );
                }
                console.warn(
                    `[withDeadline] ${label}: connection error (${consecutiveConnectionErrors}/3 before abort): ${error}`
                );
            } else {
                consecutiveConnectionErrors = 0;
                const remaining = ((timeoutMs - elapsed) / 1000).toFixed(0);
                console.warn(`[withDeadline] ${label}: poll error (${remaining}s remaining): ${error}`);
            }
        }

        await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
}
