import { generateRealisticLoad } from './wait-for-batches';
import { getL1BatchNumber } from './rpc-utils';

async function waitForL1Batch(chainName: string, timeoutMs: number = 300000): Promise<void> {
    console.log(`⏳ Waiting for L1 batch to be executed for chain: ${chainName}`);

    const startTime = Date.now();
    const maxWaitTime = timeoutMs;

    while (Date.now() - startTime < maxWaitTime) {
        try {
            const l1BatchNumber = await getL1BatchNumber(chainName);

            if (l1BatchNumber && l1BatchNumber > 0) {
                console.log(`✅ L1 batch executed successfully. Batch number: ${l1BatchNumber}`);
                return;
            }

            console.log(`⏳ Waiting for L1 batch... Current batch number: ${l1BatchNumber || 0}`);
            await new Promise((resolve) => setTimeout(resolve, 5000));
        } catch (error) {
            console.log(`⚠️ Error checking L1 batch status, retrying... Error: ${error}`);
            await new Promise((resolve) => setTimeout(resolve, 5000));
        }
    }

    throw new Error(`Timeout waiting for L1 batch execution after ${timeoutMs}ms`);
}

export async function generateLoad(chainName: string): Promise<void> {
    console.log(`🚀 Starting load generation for chain: ${chainName}`);

    try {
        console.log(`⏳ Running ETH token checks integration test`);
        await generateRealisticLoad(chainName);
        console.log(`✅ ETH token checks integration test completed`);

        console.log(`⏳ Waiting for L1 batch execution`);
        await waitForL1Batch(chainName);
        console.log(`✅ L1 batch execution completed`);

        console.log(`✅ Load generation completed successfully for chain: ${chainName}`);
    } catch (error) {
        console.error(`❌ Load generation failed for chain: ${chainName}`, error);
        throw error;
    }
}
