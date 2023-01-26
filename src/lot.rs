use crate::bucket::Bucket;

use apca::api::v2::order as apcaOrder;
use chrono::DateTime;
use chrono::Utc;
use num_decimal::Num;
use serde::{Deserialize, Serialize};
use std::error::Error;
#[cfg(test)]
use turbosql::execute;
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
    pub created_at: Option<DateTime<Utc>>, // TODO: need to track filled_at too
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
    /// Average price at which the lot was actually sold or covered.
    pub disposed_fill_price: Option<Num>,
    /// Reason for disposal
    pub dispose_reason: Option<DisposeReason>,
    /// The current status on the broker system, as of the last update
    pub broker_status: Option<apcaOrder::Status>,
    /// ID of the opening order in the broker system
    pub open_order_id: Option<apcaOrder::Id>,
    /// ID of the closing order in the broker system
    pub disposing_order_id: Option<apcaOrder::Id>,
    /// ID of the stop order in the broker system
    pub stop_order_id: Option<apcaOrder::Id>,
    /// ID of the target order in the broker system
    pub target_order_id: Option<apcaOrder::Id>,
    /// ID of the bucket
    pub bucket_id: Option<i64>,
}

impl Lot {
    pub fn create(
        sym: String,
        qty: Num,
        position_type: PositionType,
        bucket: Bucket,
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
            bucket_id: bucket.rowid,
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

    pub fn detect_disposal<F>(
        &mut self,
        order: &apcaOrder::Order,
        order_type: apcaOrder::Type,
        reason: DisposeReason,
        field_fill: F,
    ) -> Result<(), Box<dyn Error>>
    where
        F: FnOnce(&mut Lot, apcaOrder::Order),
    {
        let disposing_order = order
            .legs
            .clone()
            .into_iter()
            .filter(|leg| leg.type_ == order_type)
            .next();

        if let Some(disposing_order) = disposing_order {
            match disposing_order.status {
                apcaOrder::Status::Filled | apcaOrder::Status::PartiallyFilled => {
                    tracing::debug!(
                        "detect_disposal: {:?} lot {:?} order {:?}",
                        reason,
                        self.client_id,
                        disposing_order.id
                    );
                    self.status = Some(LotStatus::Disposed);
                    self.disposed_at = disposing_order.filled_at;
                    self.disposed_fill_price = disposing_order.average_fill_price.clone();
                    self.dispose_reason = Some(reason);
                    self.disposing_order_id = Some(disposing_order.id);
                }
                _ => {}
            }
            field_fill(self, disposing_order);
        }
        Ok(())
    }

    pub fn fill_with(&mut self, order: &apcaOrder::Order) -> Result<&mut Self, turbosql::Error> {
        let qty = order.filled_quantity.clone();

        self.set_status_from(&order.status);
        self.open_order_id = Some(order.id);
        self.limit_price = order.limit_price.clone();

        match order.status {
            apcaOrder::Status::Filled | apcaOrder::Status::PartiallyFilled => {
                tracing::debug!(
                    "fill_with: {:?} {:?} {:?}",
                    order.status,
                    self.sym,
                    order.id
                );
                self.filled_avg_price = order.average_fill_price.clone();
                self.set_cost_basis(&qty, &order.average_fill_price);
            }
            // TODO: Expired, Rejected
            _ => {
                tracing::debug!(
                    "lot::fill_with: order {} status {:?}",
                    order.id.to_string(),
                    order.status
                );
            }
        }

        // a bracket order has a stop leg and a limit leg. The original order is already
        // filled, so need to check each to see if either target was hit or stop was hit.
        if &order.legs.len() > &0 {
            self.detect_disposal(
                order,
                apcaOrder::Type::Stop,
                DisposeReason::StopOut,
                &mut |lot: &mut Lot, order: apcaOrder::Order| {
                    lot.stop_order_id = Some(order.id);
                },
            )
            .unwrap();

            self.detect_disposal(
                order,
                apcaOrder::Type::Limit,
                DisposeReason::Profit,
                &mut |lot: &mut Lot, order: apcaOrder::Order| {
                    lot.target_order_id = Some(order.id);
                },
            )
            .unwrap();
        };
        self.update()?;
        Ok(self)
    }

    pub fn liquidate_with(
        &mut self,
        order: &apcaOrder::Order,
    ) -> Result<&mut Self, turbosql::Error> {
        self.disposed_at = Some(Utc::now());
        self.disposing_order_id = Some(order.id);
        self.dispose_reason = Some(DisposeReason::Liquidation);
        self.disposed_fill_price = order.average_fill_price.clone();
        self.status = LotStatus::Disposed.into();
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

    pub fn set_status_from(&mut self, status: &apcaOrder::Status) {
        self.broker_status = Some(status.clone());
        self.status = match status {
            apcaOrder::Status::New
            | apcaOrder::Status::PendingNew
            | apcaOrder::Status::Accepted => Some(LotStatus::Pending),
            apcaOrder::Status::PartiallyFilled | apcaOrder::Status::Filled => Some(LotStatus::Open),
            apcaOrder::Status::Canceled
            | apcaOrder::Status::Rejected
            | apcaOrder::Status::Expired => Some(LotStatus::Canceled),
            _ => Some(LotStatus::Other), // this should never happen so going to flag these for
                                         // manual followup
        }
    }

    pub fn get_lots(
        page: i64,
        limit: i64,
        show_canceled: bool,
    ) -> Result<Vec<Lot>, Box<dyn Error>> {
        let lots = if show_canceled {
            select!(Vec<Lot> "ORDER BY rowid DESC LIMIT ? OFFSET ?", limit, page * limit)?
        } else {
            select!(
                Vec<Lot>
                "WHERE status = ? OR status = ? OR status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
                LotStatus::Open,
                LotStatus::Pending,
                LotStatus::Disposed,
                limit,
                page * limit
            )?
        };
        Ok(lots)
    }
}

#[cfg(test)]
fn setup() {
    let _res = std::panic::catch_unwind(|| execute!("DELETE FROM lot").unwrap());
}

#[cfg(test)]
fn create_lot() -> Lot {
    let bucket = Bucket::new("test");
    let rowid = Lot::create(
        "TEST".to_string(),
        Num::from(11),
        PositionType::Long,
        bucket,
        Some(Num::from(101)),
        Some(Num::from(102)),
        Some(Num::from(99)),
        Some(OrderTimeInForce::Day),
    );
    select!(Lot "WHERE rowid = ?", rowid).unwrap()
}

#[cfg(test)]
fn apca_order() -> apcaOrder::Order {
    apcaOrder::Order {
        id: apcaOrder::Id(Uuid::new_v4()),
        client_order_id: "c4390a00-cc88-4979-840c-7feeb08278c5".to_string(),
        status: apcaOrder::Status::Filled,
        created_at: chrono::Utc::now(),
        updated_at: Some(chrono::Utc::now()),
        submitted_at: Some(chrono::Utc::now()),
        filled_at: Some(chrono::Utc::now()),
        expired_at: None,
        canceled_at: None,
        asset_class: apca::api::v2::asset::Class::UsEquity,
        asset_id: apca::api::v2::asset::Id(Uuid::new_v4()),
        symbol: "TEST".to_string(),
        amount: apcaOrder::Amount::quantity(100),
        filled_quantity: Num::from(100),
        type_: apcaOrder::Type::Limit,
        class: apcaOrder::Class::Bracket,
        side: apcaOrder::Side::Buy,
        time_in_force: apcaOrder::TimeInForce::UntilCanceled,
        stop_price: Some(Num::from(0)),
        limit_price: Some(Num::from(0)),
        trail_price: None,
        trail_percent: None,
        average_fill_price: Some(Num::from(101)),
        extended_hours: false,
        legs: vec![],
    }
}

#[cfg(test)]
fn apca_bracket_order() -> apcaOrder::Order {
    let mut order = apca_order();

    let mut stop = apca_order();
    stop.side = apcaOrder::Side::Sell;
    stop.type_ = apcaOrder::Type::Stop;
    stop.filled_quantity = Num::from(0);
    stop.filled_at = None;
    stop.stop_price = Some(Num::from(99));
    stop.status = apcaOrder::Status::Held;

    let mut limit = apca_order();
    limit.side = apcaOrder::Side::Sell;
    limit.type_ = apcaOrder::Type::Limit;
    limit.filled_quantity = Num::from(0);
    limit.filled_at = None;
    limit.limit_price = Some(Num::from(103));
    limit.status = apcaOrder::Status::New;

    order.legs = vec![limit, stop];
    order
}

#[test]
fn test_fill_with() {
    setup();

    let order = apca_bracket_order();

    let mut lot = create_lot();

    lot.fill_with(&order).unwrap();
    assert_eq!(lot.filled_avg_price, order.average_fill_price);
    assert_eq!(lot.open_order_id.unwrap(), order.id);
    assert_eq!(lot.limit_price, order.limit_price);
    assert_eq!(lot.cost_basis.unwrap(), Num::from(10100));
    assert_eq!(lot.target_order_id.unwrap(), order.legs[0].id);
    assert_eq!(lot.stop_order_id.unwrap(), order.legs[1].id);
}

#[test]
fn test_fill_with_when_bracket_order_stopped_out() {
    let stopped_out_at = chrono::Utc::now();

    let mut order = apca_bracket_order();
    order.legs[0].status = apcaOrder::Status::Replaced;
    order.legs[1].status = apcaOrder::Status::Filled;
    order.legs[1].filled_quantity = Num::from(100);
    order.legs[1].filled_at = Some(stopped_out_at);

    let mut lot = create_lot();

    lot.fill_with(&order).unwrap();
    let stop_leg = &order.legs[1];
    assert_eq!(lot.filled_avg_price, order.average_fill_price);
    assert_eq!(lot.stop_order_id.unwrap(), stop_leg.id);
    assert_eq!(lot.disposed_at.unwrap(), stopped_out_at);
    assert_eq!(
        lot.disposed_fill_price.as_ref(),
        stop_leg.average_fill_price.as_ref()
    );
    assert_eq!(lot.dispose_reason, Some(DisposeReason::StopOut));
    assert_eq!(lot.disposing_order_id, Some(stop_leg.id));
}

#[test]
fn test_fill_with_when_bracket_order_target_hit() {
    let closed_at = chrono::Utc::now();

    let mut order = apca_bracket_order();
    order.legs[1].status = apcaOrder::Status::Replaced;
    order.legs[0].status = apcaOrder::Status::Filled;
    order.legs[0].filled_quantity = Num::from(100);
    order.legs[0].filled_at = Some(closed_at);

    let mut lot = create_lot();

    lot.fill_with(&order).unwrap();
    let target_leg = &order.legs[0];
    assert_eq!(lot.filled_avg_price, order.average_fill_price);
    assert_eq!(lot.target_order_id.unwrap(), target_leg.id);
    assert_eq!(lot.disposed_at.unwrap(), closed_at);
    assert_eq!(
        lot.disposed_fill_price.as_ref(),
        target_leg.average_fill_price.as_ref()
    );
    assert_eq!(lot.dispose_reason, Some(DisposeReason::Profit));
    assert_eq!(lot.disposing_order_id, Some(target_leg.id));
}
