use futures_util::StreamExt;
use superchain_client::{
    provider::FuelProvider, query::Bound, requests::fuel::GetSparkOrderRequest, ClientBuilder, Format, WsProvider
};
use log::{info, error};

pub struct WebSocketClientSuperchain {
    pub endpoint: String,
}

impl WebSocketClientSuperchain {
    pub fn new(endpoint: String) -> Self {
        WebSocketClientSuperchain { endpoint }
    }

    pub async fn connect(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = ClientBuilder::default()
            .credential("user","password" )
            .endpoint(&self.endpoint)
            .build::<WsProvider>()
            .await
            .map_err(|e| {
                error!("Failed to build client: {:?}", e);
                Box::new(e) as Box<dyn std::error::Error>
            })?;

        let request = GetSparkOrderRequest {
            from_block: Bound::Exact(8407000),
            to_block: Bound::None,

            ..Default::default()
        };

        let stream = client
            .get_fuel_spark_orders_by_format(request, Format::JsonStream, false)
            .await
            .map_err(|e| {
                error!("Failed to get spark orders: {:?}", e);
                Box::new(e) as Box<dyn std::error::Error>
            })?;
        superchain_client::futures::pin_mut!(stream);

        while let Some(data) = stream.next().await {
            match data {
                Ok(order_data) => {
                    let data_str = String::from_utf8(order_data).unwrap_or_default();
                    info!("=========");
                    info!("Received order data: {:?}", data_str);
                    info!("=========");
                }
                Err(e) => {
                    error!("Error receiving data from stream: {:?}", e);
                }
            }
        }

        Ok(())
    }
}
