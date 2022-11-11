use apca::data::v2::last_quote::Quote;
use chrono::DateTime;
use chrono::Utc;
use num_decimal::Num;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

type Db = Arc<RwLock<HashMap<Uuid, Position>>>;

#[derive(Debug, Serialize, Clone)]
pub struct Position {
    pub created_at: DateTime<Utc>,
    id: Uuid,
    broker_id: Option<apca::api::v2::order::Id>,
    sym: String,
    status: Option<apca::api::v2::order::Status>,
    qty: Num,
    filled_avg_price: Option<Num>,
    buy_limit: Option<Num>,
    target: Option<Num>,
    stop: Option<Num>,
    placed: bool,
    cost_basis: Option<Num>,
}

impl Default for Position {
    fn default() -> Self {
        // this does nothing useful and should be removed?
        Self {
            id: Uuid::new_v4(),
            broker_id: None,
            created_at: Utc::now(),
            status: None,
            placed: false,
            qty: Num::from(0),
            target: Some(Num::from(0)),
            filled_avg_price: Some(Num::from(0)),
            buy_limit: Some(Num::from(0)),
            sym: "".to_string(),
            stop: Some(Num::from(0)),
            cost_basis: None,
        }
    }
}

impl From<&apca::api::v2::order::Order> for Position {
    fn from(order: &apca::api::v2::order::Order) -> Position {
        let qty = order.filled_quantity.clone();

        // this would be much better as a method called on the Struct imo,
        // but apparently the only way would be this, another wrapping layer
        // of redundancy -> https://stackoverflow.com/questions/36159031/add-value-of-a-method-to-serde-serialization-output
        let cost_basis = if let Some(price) = &order.average_fill_price {
            Some(price * &qty)
        } else {
            None
        };

        let mut stop = None::<Num>;
        let mut target = None::<Num>;

        if &order.legs.len() > &0 {
            stop = order
                .legs
                .clone()
                .into_iter()
                .filter(|leg| leg.type_ == apca::api::v2::order::Type::Stop)
                .map(|leg| leg.stop_price)
                .next()
                .unwrap_or(None::<Num>);
            target = order
                .legs
                .clone()
                .into_iter()
                .filter(|leg| leg.type_ == apca::api::v2::order::Type::Limit)
                .map(|leg| leg.limit_price)
                .next()
                .unwrap_or(None::<Num>);
        };

        Position {
            broker_id: Some(order.id),
            status: Some(order.status),
            placed: true,
            created_at: order.created_at,
            buy_limit: order.limit_price.clone(),
            sym: order.symbol.clone(),
            filled_avg_price: order.average_fill_price.clone(),
            stop,
            target,
            cost_basis,
            qty,
            ..Position::default()
        }
    }
}
