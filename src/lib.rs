pub mod sync_lots;

use chrono::DateTime;
use chrono::Utc;
use num_decimal::Num;
use serde::{Deserialize, Serialize};
use std::error::Error;
use turbosql::{select, ToSql, ToSqlOutput, Turbosql};
use uuid::Uuid;

/// The status a lot can have.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum LotStatus {
    /// The order is awaiting fulfillment or partial fullfillment.
    Pending,
    /// The order is either awaiting execution or filled and is a held position.
    Open,
    /// The lot was sold or bought to cover and is final.
    Disposed,
    /// The order expired or was canceled before it was filled.
    Canceled,
    /// One of the other statuses, needs manual followup.
    Other,
}

/// needs to be implemented for any enum that is used in `select!` macro params.
// Need to make this a derive macro, but I've already spent way too much time on this, and sqlite
// is temporary anyway.
impl ToSql for LotStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, turbosql::rusqlite::Error> {
        Ok(ToSqlOutput::from(serde_json::json!(self).to_string()))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum DisposeReason {
    /// The lot was manually disposed of.
    Liquidation,
    /// The stop was hit.
    StopOut,
    /// The take profit was hit.
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
    /// Our local ID for the lot.
    pub client_id: Option<String>,
    /// Time original order was submitted.
    pub created_at: Option<DateTime<Utc>>,
    /// Symbol of the position
    pub sym: Option<String>,
    /// Number of shares or contracts or coins
    pub qty: Option<Num>,
    /// Long or Short
    pub position_type: Option<PositionType>,
    /// Whether the Lot is new, being held, or has been disposed
    pub status: Option<LotStatus>,
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
    pub cost_basis: Option<Num>,
    /// Time order was sold or covered.
    pub disposed_at: Option<DateTime<Utc>>,
    /// Price at which the lot was requested to be sold or covered.
    pub disposed_stop_price: Option<Num>,
    /// Average price at which the lot was actually sold or covered.
    pub disposed_avg_price: Option<Num>,
    /// Reason for disposal
    pub dispose_reason: Option<DisposeReason>,
    /// The current status on the broker system, as of the last update
    pub broker_status: Option<apca::api::v2::order::Status>,
    /// ID of the opening order in the broker system
    pub open_order_id: Option<apca::api::v2::order::Id>,
    /// ID of the closing order in the broker system
    pub disposing_order_id: Option<apca::api::v2::order::Id>,
    /// ID of the stop order in the broker system
    pub stop_order_id: Option<apca::api::v2::order::Id>,
    /// ID of the target order in the broker system
    pub target_order_id: Option<apca::api::v2::order::Id>,
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
            client_id: Uuid::new_v4().to_string().into(),
            sym: Some(sym),
            qty: Some(qty),
            position_type: Some(position_type),
            status: Some(LotStatus::Pending),
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

    pub fn get_by_client_id(id: &str) -> Result<Self, Box<dyn Error>> {
        let lot = select!(Lot "WHERE client_id = ?", id)?;
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
        self.set_cost_basis(&qty, &order.average_fill_price);

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
        self.set_status_from(&order.status);
        self.update()?;
        Ok(self)
    }

    pub fn set_cost_basis(&mut self, qty: &Num, fill_price: &Option<Num>) {
        self.cost_basis = if let Some(price) = fill_price {
            Some(price * qty)
        } else {
            None
        };
    }

    pub fn set_status_from(&mut self, status: &apca::api::v2::order::Status) {
        self.broker_status = Some(status.clone());
        self.status = match status {
            apca::api::v2::order::Status::New
            | apca::api::v2::order::Status::PendingNew
            | apca::api::v2::order::Status::Accepted => Some(LotStatus::Pending),
            apca::api::v2::order::Status::PartiallyFilled
            | apca::api::v2::order::Status::Filled => Some(LotStatus::Open),
            apca::api::v2::order::Status::Canceled
            | apca::api::v2::order::Status::Rejected
            | apca::api::v2::order::Status::Expired => Some(LotStatus::Canceled),
            _ => Some(LotStatus::Other), // this should never happen so going to flag these for
                                         // manual followup
        }
    }

    pub fn get_lots(page: i64, limit: i64) -> Result<Vec<Lot>, Box<dyn Error>> {
        let lots = select!(
            Vec<Lot>
            "WHERE status = ? OR status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            LotStatus::Open,
            LotStatus::Pending,
            limit,
            page * limit
        )?;
        Ok(lots)
    }
}
