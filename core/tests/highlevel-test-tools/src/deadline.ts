/**
 * Polls `fn` until it returns a non-null/non-undefined value, or the deadline expires.
 *
 * @param fn - Async function that returns `T` when the condition is met, or `null`/`undefined` to keep waiting.
 *             If `fn` throws, the error is logged and treated as "not ready yet" (retried).
 * @param opts.timeoutMs - Maximum wall-clock time to wait before throwing.
 * @param opts.intervalMs - Sleep between polls (default 2000).
 * @param opts.label - Human-readable context included in the timeout error.
 * @returns The first non-null/non-undefined value returned by `fn`.
 */
export async function withDeadline<T>(
    fn: () => Promise<T | null | undefined>,
    opts: { timeoutMs: number; intervalMs?: number; label: string }
): Promise<T> {
    const { timeoutMs, intervalMs = 2000, label } = opts;
    const start = Date.now();

    while (true) {
        const elapsed = Date.now() - start;
        if (elapsed >= timeoutMs) {
            throw new Error(`[withDeadline] ${label}: timed out after ${(elapsed / 1000).toFixed(1)}s (limit: ${(timeoutMs / 1000).toFixed(0)}s)`);
        }

        try {
            const result = await fn();
            if (result !== null && result !== undefined) {
                return result;
            }
        } catch (error) {
            const remaining = ((timeoutMs - elapsed) / 1000).toFixed(0);
            console.warn(`[withDeadline] ${label}: poll error (${remaining}s remaining): ${error}`);
        }

        await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
}
