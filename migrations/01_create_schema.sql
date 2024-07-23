CREATE TABLE IF NOT EXISTS transactions (
    transaction_id SERIAL PRIMARY KEY,
    amount TEXT NOT NULL,
    order_received_time TEXT NOT NULL, 
    match_time TEXT,
    posting_time TEXT,
    blockchain_confirmation_time TEXT,
    gas_used INT,
    details JSONB
);

CREATE INDEX idx_order_received_time ON transactions(order_received_time);

CREATE INDEX idx_match_time ON transactions(match_time);

CREATE INDEX idx_details ON transactions USING gin (details);
