DELETE FROM tokens
WHERE l1_address = decode('fC385A1dF85660a7e041423DB512f779070FCede', 'hex')
  AND l2_address = decode('C967dabf591B1f4B86CFc74996EAD065867aF19E', 'hex');