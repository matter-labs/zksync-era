import { WebServer } from '@/build-app';
import { z } from 'zod';
import { getUserByToken } from '@/query/user';
import { HttpError } from '@/errors';
import { RpcCallHandler } from '@/rpc/rpc-service';
import { allHandlers } from '@/rpc/rpc-method-handlers';

const rpcSchema = { schema: { params: z.object({ token: z.string() }) } };

export function rpcRoutes(app: WebServer) {
    app.post('/:token', rpcSchema, async (req, reply) => {
        const user = await getUserByToken(app.context.db, req.params.token).then((maybe) =>
            maybe.expect(new HttpError('Unauthorized', 401))
        );

        const handler = new RpcCallHandler(allHandlers, {
            currentUser: user.address,
            targetRpcUrl: app.context.targetRpc,
            authorizer: app.context.authorizer
        });
        reply.header('content-type', 'application/json');
        const handlerResponse = await handler.handle(req.body);
        return reply.send(handlerResponse);
    });
}
