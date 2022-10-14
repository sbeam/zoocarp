use apca::data::v2::last_quote::Quote;
use chrono::DateTime;
use chrono::Utc;
use num_decimal::Num;
use serde::Serialize;
use uuid::Uuid;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/* This is needed because apca's Quote does not derive Serialize, and I'd like it to.
 * There might be a better way to do this, it feels very Java-like */
#[derive(Debug, Serialize)]
pub struct SerializableEntityQuote {
    pub time: DateTime<Utc>,
    pub ask_price: Num,
    pub ask_size: u64,
    pub bid_price: Num,
    pub bid_size: u64,
}

impl From<Quote> for SerializableEntityQuote {
    fn from(quote: Quote) -> SerializableEntityQuote {
        SerializableEntityQuote {
            time: quote.time,
            ask_price: quote.ask_price,
            ask_size: quote.ask_size,
            bid_price: quote.bid_price,
            bid_size: quote.bid_size,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Position {
    id: Uuid,
    sym: String,
    status: Option<apca::api::v2::order::Status>,
    qty: Num,
    opened: DateTime<Utc>,
    filled_avg_price: Num,
    buy_limit: Num,
    target: Num,
    placed: bool,
    cost_basis: Option<Num>,
}

impl Default for Position {
    fn default() -> Self {
        // this does nothing useful and should be removed?
        Self {
            id: Uuid::new_v4(),
            opened: Utc::now(),
            status: None,
            placed: false,
            qty: Num::from(0),
            target: Num::from(0),
            filled_avg_price: Num::from(0),
            buy_limit: Num::from(0),
            sym: "".to_string(),
            cost_basis: None,
        }
    }
}

impl From<&apca::api::v2::order::Order> for Position {
    fn from(order: &apca::api::v2::order::Order) -> Position {
        let qty = order.filled_quantity.clone();

        let cost_basis = if let Some(price) = &order.average_fill_price {
            Some(price * &qty)
        } else {
            None
        };

        Position {
            status: Some(order.status),
            cost_basis,
            qty,
            ..Position::default()
        }
    }
}

impl Position {
    fn cost_basis(self) -> Num {
        self.filled_avg_price * Num::from(self.qty)
    }
}

type Db = Arc<RwLock<HashMap<Uuid, Position>>>;
