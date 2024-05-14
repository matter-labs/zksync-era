ALTER TABLE l1_batches ADD l1_gas_price_u256 numeric(80,0) NOT NULL DEFAULT 0;
ALTER TABLE l1_batches ADD l2_fair_gas_price_u256 numeric(80,0) NOT NULL DEFAULT 0;

ALTER TABLE miniblocks ADD l1_gas_price_u256 numeric(80,0) NOT NULL DEFAULT 0;
ALTER TABLE miniblocks ADD l2_fair_gas_price_u256 numeric(80,0) NOT NULL DEFAULT 0;
ALTER TABLE miniblocks ADD fair_pubdata_price_u256 numeric(80,0) NOT NULL DEFAULT 0;

UPDATE l1_batches SET l1_gas_price_u256=l1_gas_price;
UPDATE l1_batches SET l2_fair_gas_price_u256=l2_fair_gas_price;

UPDATE miniblocks SET l1_gas_price_u256=l1_gas_price;
UPDATE miniblocks SET l2_fair_gas_price_u256=l2_fair_gas_price;
UPDATE miniblocks SET fair_pubdata_price_u256=fair_pubdata_price;

CREATE FUNCTION update_l1_gas_price_old_column() RETURNS trigger AS $update_l1_gas_price_old_column$
    BEGIN
        NEW.l1_gas_price := cast(NEW.l1_gas_price_u256 AS bigint);
        RETURN NEW;
    exception WHEN numeric_value_out_of_range THEN
        NEW.l1_gas_price := -1;
        RETURN NEW;
    END;
$update_l1_gas_price_old_column$ LANGUAGE plpgsql;


CREATE FUNCTION update_l2_fair_gas_price_old_column() RETURNS trigger AS $update_l2_fair_gas_price_old_column$
    BEGIN
        NEW.l2_fair_gas_price := cast(NEW.l2_fair_gas_price_u256 AS bigint);
        RETURN NEW;
    exception WHEN numeric_value_out_of_range THEN
        NEW.l2_fair_gas_price := -1;
        RETURN NEW;
    END;
$update_l2_fair_gas_price_old_column$ LANGUAGE plpgsql;

CREATE FUNCTION update_fair_pubdata_price_old_column() RETURNS trigger AS $update_fair_pubdata_price_old_column$
    BEGIN
        NEW.fair_pubdata_price := cast(NEW.fair_pubdata_price_u256 AS bigint);
        RETURN NEW;
    exception WHEN numeric_value_out_of_range THEN
        NEW.fair_pubdata_price := -1;
        RETURN NEW;
    END;
$update_fair_pubdata_price_old_column$ LANGUAGE plpgsql;

CREATE TRIGGER update_l1_gas_price_old_column BEFORE INSERT OR UPDATE ON l1_batches FOR each ROW EXECUTE FUNCTION update_l1_gas_price_old_column();
CREATE TRIGGER update_l2_fair_gas_price_old_column BEFORE INSERT OR UPDATE ON l1_batches FOR each ROW EXECUTE FUNCTION update_l2_fair_gas_price_old_column();

CREATE TRIGGER update_l1_gas_price_old_column BEFORE INSERT OR UPDATE ON miniblocks FOR each ROW EXECUTE FUNCTION update_l1_gas_price_old_column();
CREATE TRIGGER update_l2_fair_gas_price_old_column BEFORE INSERT OR UPDATE ON miniblocks FOR each ROW EXECUTE FUNCTION update_l2_fair_gas_price_old_column();
CREATE TRIGGER update_fair_pubdata_price_old_column BEFORE INSERT OR UPDATE ON miniblocks FOR each ROW EXECUTE FUNCTION update_fair_pubdata_price_old_column();
