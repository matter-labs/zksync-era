ALTER TABLE events
    ADD FOREIGN KEY (miniblock_number) REFERENCES miniblocks;
ALTER TABLE l2_to_l1_logs
    ADD FOREIGN KEY (miniblock_number) REFERENCES miniblocks;
