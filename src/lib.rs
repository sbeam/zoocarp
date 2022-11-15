use chrono::DateTime;
use chrono::Utc;
use num_decimal::Num;
use serde::{Deserialize, Serialize};
use std::error::Error;
use turbosql::{execute, select, Turbosql};

/// The status a position can have.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Status {
    New,
    Held,
    Disposed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum DisposeReason {
    Liquidation,
    StopOut,
    Profit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PositionType {
    #[default]
    Long,
    Short,
}

/// A description of the time for which an order is valid.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum OrderTimeInForce {
    /// The order is good for the day, and it will be canceled
    /// automatically at the end of Regular Trading Hours if unfilled.
    #[serde(rename = "day")]
    Day,
    /// The order is good until canceled.
    #[serde(rename = "gtc")]
    UntilCanceled,
}

#[derive(Debug, Serialize, Turbosql, Default, Clone)]
pub struct Lot {
    /// DB row ID
    pub rowid: Option<i64>,
    /// Time original order was submitted.
    pub created_at: Option<DateTime<Utc>>,
    /// Symbol of the position
    pub sym: Option<String>,
    /// Number of shares or contracts or coins
    pub qty: Option<Num>,
    /// Long or Short
    pub position_type: Option<PositionType>,
    /// Whether the Lot is new, being held, or has been disposed
    pub status: Option<Status>,
    /// How long the order should remain active
    pub time_in_force: Option<OrderTimeInForce>,
    /// Average price of the lot per unit
    pub filled_avg_price: Option<Num>,
    /// Original buy/sell limit price as entered by the user
    pub limit_price: Option<Num>,
    /// Original take profit price as entered by the user
    pub target_price: Option<Num>,
    /// Original stop loss price as entered by the user
    pub stop_price: Option<Num>,
    /// Total cost basis for the lot
    cost_basis: Option<Num>,
    /// Time order was sold or covered.
    pub disposed_at: Option<DateTime<Utc>>,
    /// Price at which the lot was requested to be sold or covered.
    disposed_stop_price: Option<Num>,
    /// Average price at which the lot was actually sold or covered.
    disposed_avg_price: Option<Num>,
    /// Reason for disposal
    dispose_reason: Option<DisposeReason>,
    /// ID of the opening order in the broker system
    open_order_id: Option<apca::api::v2::order::Id>,
    /// ID of the closing order in the broker system
    disposing_order_id: Option<apca::api::v2::order::Id>,
    /// ID of the stop order in the broker system
    stop_order_id: Option<apca::api::v2::order::Id>,
    /// ID of the target order in the broker system
    target_order_id: Option<apca::api::v2::order::Id>,
}

impl Lot {
    pub fn create(
        sym: String,
        qty: Num,
        position_type: PositionType,
        limit_price: Option<Num>,
        target_price: Option<Num>,
        stop_price: Option<Num>,
        time_in_force: Option<OrderTimeInForce>,
    ) -> i64 {
        let lot = Self {
            created_at: Some(Utc::now()),
            sym: Some(sym),
            qty: Some(qty),
            position_type: Some(position_type),
            status: Some(Status::New),
            time_in_force,
            limit_price,
            target_price,
            stop_price,
            ..Default::default()
        };
        lot.insert().unwrap()
    }

    pub fn get(rowid: i64) -> Result<Self, Box<dyn Error>> {
        let lot = select!(Lot "WHERE rowid = ?", rowid)?;
        Ok(lot)
    }

    pub fn fill_with(
        &mut self,
        order: &apca::api::v2::order::Order,
    ) -> Result<&mut Self, turbosql::Error> {
        let qty = order.filled_quantity.clone();

        self.open_order_id = Some(order.id);
        self.limit_price = order.limit_price.clone();
        self.filled_avg_price = order.average_fill_price.clone();
        self.cost_basis = if let Some(price) = &order.average_fill_price {
            Some(price * &qty)
        } else {
            None
        };

        if &order.legs.len() > &0 {
            let stop_order = order
                .legs
                .clone()
                .into_iter()
                .filter(|leg| leg.type_ == apca::api::v2::order::Type::Stop)
                .next();
            self.stop_order_id = Some(stop_order.unwrap().id);

            let target_order = order
                .legs
                .clone()
                .into_iter()
                .filter(|leg| leg.type_ == apca::api::v2::order::Type::Limit)
                .next();
            self.target_order_id = Some(target_order.unwrap().id);
        };
        self.status = match order.status {
            apca::api::v2::order::Status::New => Some(Status::New),
            apca::api::v2::order::Status::PartiallyFilled => Some(Status::Held),
            apca::api::v2::order::Status::Filled => Some(Status::Held),
            apca::api::v2::order::Status::Canceled => Some(Status::Disposed),
            apca::api::v2::order::Status::Rejected => Some(Status::Disposed),
            apca::api::v2::order::Status::Expired => Some(Status::Disposed),
            _ => None,
        };
        self.update()?;
        Ok(self)
    }

    pub fn get_lots(page: i64, limit: i64) -> Result<Vec<Lot>, Box<dyn Error>> {
        let lots = select!(
            Vec<Lot>
            "ORDER BY created_at DESC LIMIT ? OFFSET ?",
            limit,
            page * limit
        )?;
        Ok(lots)
    }
}
