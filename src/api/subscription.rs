
use crate::config::ev;
use crate::model::OrderType;


use log::info;

pub fn format_graphql_subscription(order_type: OrderType) -> String {
    let limit = ev("FETCH_ORDER_LIMIT").unwrap_or_default();
    let market = ev("CONTRACT_ID").unwrap_or_default();  
    let order_type_str = match order_type {
        OrderType::Sell => "ActiveSellOrder",
        OrderType::Buy => "ActiveBuyOrder",
    };

    let qe = format!(
        r#"query MyQuery {{
            {}(limit: {}, where: {{market: {{_eq: "{}"}}}}) {{
                id
                user
                timestamp
                order_type
                amount
                asset
                price
                status
                db_write_timestamp
                initial_amount
            }}
        }}"#,
        order_type_str, limit, market
    );
    info!("debug query");
    info!("=======");
    info!("{:?}", &qe);
    info!("=======");
    qe
}

