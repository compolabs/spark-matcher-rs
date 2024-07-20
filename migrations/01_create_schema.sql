CREATE TABLE IF NOT EXISTS transactions (
    transaction_id SERIAL PRIMARY KEY,
    amount DECIMAL NOT NULL,
    order_received_time TIMESTAMP WITH TIME ZONE NOT NULL, -- Время получения ордера, должно быть указано при вставке
    match_time TIMESTAMP WITH TIME ZONE,
    posting_time TIMESTAMP WITH TIME ZONE,
    blockchain_confirmation_time TIMESTAMP WITH TIME ZONE,
    gas_used INT,
    details JSONB
);

CREATE INDEX idx_order_received_time ON transactions(order_received_time);

CREATE INDEX idx_match_time ON transactions(match_time);

CREATE INDEX idx_details ON transactions USING gin (details);
