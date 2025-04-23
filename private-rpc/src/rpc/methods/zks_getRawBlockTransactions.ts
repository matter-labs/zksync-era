import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { z } from 'zod';
import { addressSchema } from '@/schemas/address';
import { isAddressEqual } from 'viem';
import { sendToTargetRpc } from '@/rpc/methods/utils';

const schema = z
    .object({
        jsonrpc: z.literal('2.0'),
        id: z.any(),
        result: z.array(
            z
                .object({
                    common_data: z
                        .object({
                            L2: z.object({ initiatorAddress: addressSchema }).passthrough()
                        })
                        .passthrough()
                })
                .passthrough()
        )
    })
    .passthrough();

export const zks_getRawBlockTransactions: MethodHandler = {
    name: 'zks_getRawBlockTransactions',
    async handle(
        context: RequestContext,
        method: string,
        params: unknown[],
        id: number | string
    ): Promise<FastifyReplyType> {
        const data = await sendToTargetRpc(context.targetRpcUrl, id, method, params, schema);

        const filtered = data.result.filter((r) =>
            isAddressEqual(r.common_data.L2.initiatorAddress, context.currentUser)
        );

        return {
            ...data,
            result: filtered
        };
    }
};
