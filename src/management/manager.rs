use std::collections::BTreeMap;
use log::info;
use tokio::sync::RwLock;
use crate::model::{SpotOrder, OrderType};
use std::sync::Arc;

pub struct OrderManager {
    pub buy_orders: RwLock<BTreeMap<u128, Vec<SpotOrder>>>,
    pub sell_orders: RwLock<BTreeMap<u128, Vec<SpotOrder>>>,
}

impl OrderManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buy_orders: RwLock::new(BTreeMap::new()),
            sell_orders: RwLock::new(BTreeMap::new()),
        })
    }

    pub async fn add_order(&self, order: SpotOrder) {
        let mut order_map = match order.order_type {
            OrderType::Buy => self.buy_orders.write().await,
            OrderType::Sell => self.sell_orders.write().await,
        };

        order_map.entry(order.price)
                 .or_default()
                 .push(order);
    }

    pub async fn remove_order(&self, order_id: &str, price: u128, order_type: OrderType) {
        let mut order_map = match order_type {
            OrderType::Buy => self.buy_orders.write().await,
            OrderType::Sell => self.sell_orders.write().await,
        };

        if let Some(orders) = order_map.get_mut(&price) {
            orders.retain(|order| order.id != order_id);
            if orders.is_empty() {
                order_map.remove(&price);
            }
        }
    }

    pub async fn get_orders(&self, price: u128, order_type: OrderType) -> Vec<SpotOrder> {
        let order_map = match order_type {
            OrderType::Buy => self.buy_orders.read().await,
            OrderType::Sell => self.sell_orders.read().await,
        };

        order_map.get(&price).cloned().unwrap_or_else(Vec::new)
    }

    pub async fn log_orders(&self) {
        let buy_orders = self.buy_orders.read().await;
        let sell_orders = self.sell_orders.read().await;

        info!("Current Buy Orders:");
        for (price, orders) in buy_orders.iter() {
            info!("Price: {} -> Orders: {:?}", price, orders);
        }

        info!("Current Sell Orders:");
        for (price, orders) in sell_orders.iter() {
            info!("Price: {} -> Orders: {:?}", price, orders);
        }
    }
}
