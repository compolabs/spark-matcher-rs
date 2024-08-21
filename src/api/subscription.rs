use crate::config::ev;
use crate::model::OrderType;

pub fn format_graphql_subscription(order_type: OrderType) -> String {
    let limit = ev("FETCH_ORDER_LIMIT").unwrap_or_default();
    let order_type_str = match order_type {
        OrderType::Sell => "ActiveSellOrder",
        OrderType::Buy => "ActiveBuyOrder",
    };

    format!(
        r#"query MyQuery {{
            {}(limit: {}) {{
                id
                user
                timestamp
                order_type
                amount
                asset
                price
                status
                asset_type
                db_write_timestamp
                initial_amount
            }}
        }}"#,
        order_type_str, limit
    )
}
