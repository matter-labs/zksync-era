CREATE INDEX l1_batches_number_idx ON public.l1_batches USING btree (number) WHERE (is_sealed = false);
