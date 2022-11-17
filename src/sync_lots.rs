use apca::api::v2::order;
use apca::{ApiInfo, Client};
use futures::future::join_all;
use std::error::Error;
use turbosql::{execute, select, Turbosql};

use crate::{Lot, LotStatus};

pub async fn startup_sync() -> Result<(), Box<dyn Error>> {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);
    let open_lots = select!(Vec<Lot> "WHERE status != ?", LotStatus::Closed.to_string())?;
    tracing::info!("Syncing {} open lots", open_lots.len());

    join_all(open_lots.iter().map(|lot| {
        tracing::debug!("startup_sync: {:?}", lot);

        async {
            let alpaca_order = client
                .issue::<order::GetByClientId>(&lot.rowid.unwrap().to_string())
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
