use apca::data::v2::last_quote::Quote;
use chrono::DateTime;
use chrono::Utc;
use num_decimal::Num;
use serde::Serialize;

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
