INSERT INTO tokens (l1_address, l2_address, name, symbol, decimals, well_known, created_at, updated_at)
VALUES (
   decode('fC385A1dF85660a7e041423DB512f779070FCede', 'hex'),
   decode('C967dabf591B1f4B86CFc74996EAD065867aF19E', 'hex'),
   'ZKLink',
   'ZKL',
   18,
   false,
   CURRENT_TIMESTAMP,
   CURRENT_TIMESTAMP
) ON CONFLICT DO NOTHING;