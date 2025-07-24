import { WebServer } from '@/build-app';
import { z } from 'zod';
import { addressSchema } from '@/schemas/address';
import { usersTable } from '@/db/schema';
import { nanoid } from 'nanoid';
import { HttpError } from '@/errors';
import { eq } from 'drizzle-orm';

const createUserSchema = {
    schema: {
        body: z.object({
            address: addressSchema,
            secret: z.string()
        })
    }
};

export function usersRoutes(app: WebServer) {
    app.post('/', createUserSchema, async (req, reply) => {
        const { address, secret } = req.body;
        const { authorizer, db, createTokenSecret } = app.context;

        if (!authorizer.isAddressWhitelisted(address)) {
            throw new HttpError('Forbidden: Address not whitelisted', 403);
        }

        const token = nanoid(32);

        if (secret !== createTokenSecret) {
            throw new HttpError('forbidden', 403);
        }

        await db.transaction(async (tx) => {
            await tx.delete(usersTable).where(eq(usersTable.address, address));

            await tx.insert(usersTable).values({
                address,
                token
            });
        });

        return reply.send({ ok: true, token });
    });

    const getUserSchema = {
        schema: {
            params: z.object({
                address: addressSchema
            }),
            querystring: z.object({
                secret: z.string()
            })
        }
    };

    app.get('/:address', getUserSchema, (req, reply) => {
        const { authorizer, createTokenSecret } = app.context;
        const secret = req.headers['x-secret'];

        if (secret !== createTokenSecret) {
            throw new HttpError('forbidden', 403);
        }

        const authorized = authorizer.isAddressWhitelisted(req.params.address);
        return reply.send({ authorized });
    });
}
