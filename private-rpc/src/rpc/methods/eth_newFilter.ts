import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { isAddress, isAddressEqual } from 'viem';
import { z } from 'zod';
import { hexSchema } from '@/schemas/hex';
import { unauthorized } from '@/rpc/json-rpc';
import { delegateCall } from '@/rpc/delegate-call';

const schema = z
    .object({
        topics: z.array(hexSchema)
    })
    .passthrough();

export const eth_newFilter: MethodHandler = {
    name: 'eth_newFilter',
    async handle(
        context: RequestContext,
        method: string,
        params: unknown[],
        id: number | string
    ): Promise<FastifyReplyType> {
        const parsed = schema.parse(params);
        const isTargetToUser = parsed.topics.some(
            (param) => isAddress(param) && isAddressEqual(param, context.currentUser)
        );

        if (!isTargetToUser) {
            return unauthorized(id);
        }

        return delegateCall({ url: context.targetRpcUrl, id, method, params });
    }
};
