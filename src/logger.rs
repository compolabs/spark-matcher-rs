use sqlx::PgPool;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct TransactionLog {
    pub total_amount: u128,
    pub matches_len: usize,
    pub tx_id: String,
    pub gas_used: u64,
    pub match_time_ms: i64,
    pub buy_orders: usize,
    pub sell_orders: usize,
    pub receive_time_ms: i64,
    pub post_time_ms: i64,
}

pub async fn log_transactions(
    mut receiver: mpsc::UnboundedReceiver<TransactionLog>,
    db_pool: PgPool,
) {
    while let Some(log) = receiver.recv().await {
        let total_amount = log.total_amount.to_string();
        let match_time_ms = log.match_time_ms;
        let buy_orders = log.buy_orders as i32;
        let sell_orders = log.sell_orders as i32;
        let avg_gas_used = log.gas_used as i32 / log.matches_len as i32;
        let total_gas_used = log.gas_used as i32;
        let receive_time_ms = log.receive_time_ms;
        let post_time_ms = log.post_time_ms;

        sqlx::query!(
            r#"
            INSERT INTO transaction_stats (total_transactions, total_amount, avg_gas_used, total_gas_used, match_time_ms, buy_orders, sell_orders, receive_time_ms, post_time_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            log.matches_len as i32,
            total_amount,
            avg_gas_used,
            total_gas_used,
            match_time_ms,
            buy_orders,
            sell_orders,
            receive_time_ms,
            post_time_ms
        )
        .execute(&db_pool)
        .await
        .expect("Failed to log transaction");
    }
}
