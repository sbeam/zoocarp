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
struct Position {
    id: Uuid,
    sym: String,
    qty: u64,
    opened: DateTime<Utc>,
    filled_avg_price: Num,
    buy_limit: Num,
    target: Num,
    placed: bool,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            opened: Utc::now(),
            placed: false,
            qty: 0,
            target: Num::new(0, 0),
            filled_avg_price: Num::new(0, 0),
            buy_limit: Num::new(0, 0),
            sym: "".to_string(),
        }
    }
}

type Db = Arc<RwLock<HashMap<Uuid, Position>>>;
