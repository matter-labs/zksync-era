import { getL1BatchDetails, getL1BatchNumber } from './rpc-utils';
import { runIntegrationTests } from './run-integration-tests';

/**
 * Waits for all L1 batches to be executed by checking if executeTxHash becomes not null
 * @param chainName - The name of the chain to monitor
 * @param timeoutMs - Timeout in milliseconds (default: 300000 = 5 minutes)
 * @returns Promise that resolves with the L1 batch details when execution is complete
 */
export async function waitForAllBatchesToBeExecuted(chainName: string, timeoutMs: number = 300000): Promise<any> {
    const batchNumber = await getL1BatchNumber(chainName);
    console.log(`⏳ Waiting for L1 batch ${batchNumber} execution (executeTxHash to become not null)...`);

    const startTime = Date.now();
    while (Date.now() - startTime < timeoutMs) {
        try {
            const l1BatchDetails = await getL1BatchDetails(chainName, batchNumber);

            if (l1BatchDetails.executeTxHash) {
                console.log(
                    `✅ L1 batch ${batchNumber} executed successfully. ExecuteTxHash: ${l1BatchDetails.executeTxHash}`
                );
                return l1BatchDetails;
            }

            await new Promise((resolve) => setTimeout(resolve, 5000));
        } catch (error) {
            console.log(`⚠️ Error checking L1 batch execution status, retrying... Error: ${error}`);
            await new Promise((resolve) => setTimeout(resolve, 5000));
        }
    }

    throw new Error(`Timeout waiting for L1 batch ${batchNumber} execution after ${timeoutMs}ms`);
}

/**
 * Generates realistic load on the chain by running integration tests with ETH token checks
 * @param chainName - The name of the chain to generate load on
 * @returns Promise that resolves when the load generation is complete
 */
export async function generateRealisticLoad(chainName: string): Promise<void> {
    console.log(`🚀 Generating realistic load on chain: ${chainName}`);
    await runIntegrationTests(chainName, undefined, 'ETH token checks');
    console.log(`✅ Realistic load generation completed for chain: ${chainName}`);
}
