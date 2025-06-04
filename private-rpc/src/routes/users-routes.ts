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
        const token = nanoid(32);

        if (secret !== app.context.createTokenSecret) {
            throw new HttpError('forbidden', 403);
        }

        await app.context.db
            .delete(usersTable)
            .where(eq(usersTable.address, address));

        await app.context.db.insert(usersTable).values({
            address,
            token
        });

        return reply.send({ ok: true, token });
    });
}
