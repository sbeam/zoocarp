use apca::api::v2::order;
use apca::{ApiInfo, Client};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use num_decimal::Num;
use serde::Deserialize;
use std::error::Error;
use turbosql::{select, Turbosql};

use crate::{Lot, LotStatus};

#[derive(Deserialize)]
struct TradeUpdateMessageRoot {
    stream: String,
    data: TradeUpdateMessageData,
}

#[derive(Deserialize)]
struct TradeUpdateMessageData {
    event: Event,
    execution_id: String,
    timestamp: DateTime<Utc>,
    price: Num,
    qty: u32,
    order: order::Order,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
enum Event {
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
    if update_message.stream != "trade_update" {
        return Ok(());
    }
    match update_message.data.event {
        Event::Fill | Event::PartialFill => {
            let order = update_message.data.order;
            let mut lot = Lot::get_by_client_id(&order.client_order_id)?;
            tracing::debug!(
                "sync_trade_update: order filled {:?} {:?}",
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
    let open_lots = select!(
        Vec<Lot> "WHERE status != ? AND status != ? AND client_id IS NOT NULL",
        LotStatus::Canceled,
        LotStatus::Disposed
    )
    .unwrap_or_default();
    tracing::info!("Syncing {} open lots", open_lots.len());

    join_all(open_lots.iter().map(|lot| {
        tracing::debug!("startup_sync: {:?}", lot);

        async {
            let alpaca_order = client
                .issue::<order::GetByClientId>(&lot.client_id.as_ref().unwrap().clone())
                .await;
            match alpaca_order {
                Ok(order) => {
                    if lot.broker_status != Some(order.status) {
                        let mut lot = lot.clone();
                        lot.set_status_from(&order.status);
                        match order.status {
                            order::Status::Filled | order::Status::PartiallyFilled => {
                                tracing::debug!(
                                    "startup_sync: order filled {:?} {:?}",
                                    lot.sym,
                                    order.id
                                );
                                lot.qty = Some(order.filled_quantity.clone());
                                lot.filled_avg_price = order.average_fill_price.clone();
                                lot.set_cost_basis(
                                    &order.filled_quantity,
                                    &order.average_fill_price,
                                );
                            }
                            _ => {
                                tracing::debug!(
                                    "startup_sync: order {} status {:?}",
                                    order.id.to_string(),
                                    order.status
                                );
                            }
                        };
                        if lot.open_order_id.is_none() {
                            lot.open_order_id = Some(order.id);
                        }
                        lot.update().expect("failed to update lot");
                    }
                }
                Err(e) => {
                    tracing::error!("startup_sync: {:?}", e);
                }
            }
        }
    }))
    .await;
    Ok(())
}
