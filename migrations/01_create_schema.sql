CREATE TABLE IF NOT EXISTS transaction_stats (
    stat_id SERIAL PRIMARY KEY,
    total_transactions INT,
    total_amount TEXT,
    avg_gas_used INT,
    total_gas_used INT,
    match_time_ms BIGINT,
    buy_orders INT,
    sell_orders INT,
    receive_time_ms BIGINT,
    post_time_ms BIGINT
);
