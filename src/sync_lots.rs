use apca::api::v2::order;
use apca::{ApiInfo, Client};
use futures::future::join_all;
use serde::Deserialize;
use std::error::Error;
use turbosql::{select, Turbosql};

use crate::lot::{Lot, LotStatus};
use crate::update_server::UpdateNotification;

#[derive(Deserialize)]
struct TradeUpdateMessageRoot {
    stream: String,
    data: TradeUpdateMessageData,
}

#[derive(Deserialize)]
struct TradeUpdateMessageData {
    event: Event,
    // execution_id: String,
    // timestamp: DateTime<Utc>,
    // price: Num,
    // qty: u32,
    order: order::Order,
}

#[derive(Debug)]
pub struct LotUpdateNotice {
    pub sym: String,
    pub rowid: Option<i64>,
    pub order_id: Option<String>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
enum Event {
    #[serde(rename = "new")]
    New,
    #[serde(rename = "fill")]
    Fill,
    #[serde(rename = "partial_fill")]
    PartialFill,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "expired")]
    Expired,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "done_for_day")]
    DoneForDay,
    #[serde(rename = "replaced")]
    Replaced,
}

// process a trade_update message
pub fn sync_trade_update(msg: &str) -> Result<(), Box<dyn Error>> {
    let update_message: TradeUpdateMessageRoot = serde_json::from_str(msg)?;
    if update_message.stream != "trade_updates" {
        return Ok(());
    }
    match update_message.data.event {
        Event::Fill | Event::PartialFill => {
            let order = update_message.data.order;
            let mut lot = Lot::get_by_client_id(&order.client_order_id)?;
            lot.set_status_from(&order.status);
            tracing::info!(
                "sync_trade_update: order status {:?}: {:?} {:?}",
                order.status,
                lot.sym,
                order.id
            );
            lot.qty = Some(order.filled_quantity.clone());
            lot.filled_avg_price = order.average_fill_price.clone();
            lot.set_cost_basis(&order.filled_quantity, &order.average_fill_price);
            lot.update().expect("failed to update lot");
        }
        _ => {
            tracing::warn!(
                "sync_trade_update: ignoring event {:?}",
                update_message.data.event
            );
        }
    }

    Ok(())
}

pub async fn startup_sync() -> Result<(), Box<dyn Error>> {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);
    let mut open_lots = select!(
        Vec<Lot> "WHERE status != ? AND status != ? AND client_id IS NOT NULL",
        LotStatus::Canceled,
        LotStatus::Disposed
    )
    .unwrap_or_default();
    tracing::info!("Syncing {} open lots", open_lots.len());

    join_all(open_lots.iter_mut().map(|lot| {
        tracing::debug!("startup_sync: {:?}", lot);

        async {
            let alpaca_order = client
                .issue::<order::GetByClientId>(&lot.client_id.as_ref().unwrap().clone())
                .await;
            match alpaca_order {
                Ok(order) => {
                    lot.fill_with(&order)
                        .expect("failed to fill lot with order");
                }
                Err(e) => {
                    // : startup_sync: Endpoint(NotFound(Ok(ApiError { code: 40410000, message: "order not found for e131881b-d6b0-4378-a5d5-cd419c4d3d34" })))
                    match e.source() {
                        Some(source) => {
                            if source.to_string().contains("order not found") {
                                lot.status = Some(LotStatus::Canceled);
                                lot.update().expect("failed to update lot");
                            }
                        }
                        None => {}
                    }
                    tracing::error!("startup_sync: {:?}", e);
                }
            }
        }
    }))
    .await;
    Ok(())
}
