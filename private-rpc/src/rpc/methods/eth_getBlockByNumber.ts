import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { z } from 'zod';
import { sendToTargetRpc } from '@/rpc/methods/utils';

const schema = z
    .object({
        result: z.union([
            z
                .object({
                    transactions: z.any()
                })
                .passthrough(),
            z.null()
        ])
    })
    .passthrough();

export const filterTransactions: MethodHandler = {
    name: 'eth_getBlockByNumber',
    async handle(
        context: RequestContext,
        method: string,
        params: unknown[],
        id: number | string
    ): Promise<FastifyReplyType> {
        const data = await sendToTargetRpc(context.targetRpcUrl, id, method, params, schema);

        return {
            ...data,
            result: {
                ...data.result,
                transactions: []
            }
        };
    }
};

export const eth_getBlockByNumber: MethodHandler = filterTransactions;
export const eth_getBlockByHash: MethodHandler = filterTransactions;
