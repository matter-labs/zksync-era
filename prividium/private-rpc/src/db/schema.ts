import { integer, pgTable, varchar } from 'drizzle-orm/pg-core';
import { addressRow } from './custom-types';

export const usersTable = pgTable('users', {
    id: integer('id').primaryKey().generatedAlwaysAsIdentity(),
    token: varchar('token', { length: 255 }).unique().notNull(),
    address: addressRow('address').notNull()
});
