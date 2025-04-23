import { DB } from '@/db';
import { usersTable } from '@/db/schema';
import { eq } from 'drizzle-orm';
import { Option } from 'nochoices';

export async function getUserByToken(db: DB, token: string): Promise<Option<typeof usersTable.$inferSelect>> {
    const [row] = await db.select().from(usersTable).where(eq(usersTable.token, token)).limit(1);
    return Option.fromNullable(row);
}
