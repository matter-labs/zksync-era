ALTER TABLE transactions
    ADD FOREIGN KEY (miniblock_number) REFERENCES miniblocks;
