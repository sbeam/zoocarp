use apca::api::v2::order::Order;
use num_decimal::Num;
use std::fs::read_to_string;
use turbosql::{execute, select, Turbosql};
use uuid::Uuid;
use zoocarp::bucket::Bucket;
use zoocarp::lot::{Lot, LotStatus, OrderTimeInForce, PositionType};
use zoocarp::sync_lots::*;

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

// this really tests that turbosql is working, but also that ToSql hack is in place to make status selectable
#[test]
fn test_lot_can_be_saved_and_fetched() {
    setup();
    let lot = create_lot();
    assert!(lot.rowid.unwrap() > 0);
    assert_eq!(lot.sym.unwrap(), "TEST");
    assert_eq!(lot.status.unwrap(), LotStatus::Pending);

    let mut lot2 = create_lot();
    lot2.status = Some(LotStatus::Open);
    assert!(lot2.rowid.unwrap() > 1);
    lot2.update().unwrap();

    let lots = select!(Vec<Lot> "WHERE status = ?", LotStatus::Open).unwrap();

    assert_eq!(lots.len(), 1);
}

#[test]
fn test_sync_trade_update_fill() {
    setup();
    let mut lot = create_lot();
    let fixture_client_id = "c4390a00-cc88-4979-840c-7feeb08278c5".to_string();
    lot.client_id = Some(fixture_client_id.clone());
    assert_eq!(lot.status, Some(LotStatus::Pending));
    lot.update().unwrap();

    let message = read_to_string("tests/fixtures/update_fill.json").unwrap();
    let _res = sync_trade_update(&message).expect("sync_trade_update failed");

    let lot = Lot::get_by_client_id(&fixture_client_id).unwrap();
    assert_eq!(lot.qty, Some(Num::from(90)));
    assert_eq!(lot.filled_avg_price, Some(Num::new(1354, 100)));
    assert_eq!(lot.cost_basis, Some(Num::new(121860, 100)));
    assert_eq!(lot.status, Some(LotStatus::Open));
}

#[cfg(test)]
fn apca_order() -> Order {
    Order {
        id: apca::api::v2::order::Id(Uuid::new_v4()),
        client_order_id: "c4390a00-cc88-4979-840c-7feeb08278c5".to_string(),
        status: apca::api::v2::order::Status::Filled,
        created_at: chrono::Utc::now(),
        updated_at: Some(chrono::Utc::now()),
        submitted_at: Some(chrono::Utc::now()),
        filled_at: Some(chrono::Utc::now()),
        expired_at: None,
        canceled_at: None,
        asset_class: apca::api::v2::asset::Class::UsEquity,
        asset_id: apca::api::v2::asset::Id(Uuid::new_v4()),
        symbol: "TEST".to_string(),
        amount: apca::api::v2::order::Amount::quantity(100),
        filled_quantity: Num::from(100),
        type_: apca::api::v2::order::Type::Limit,
        class: apca::api::v2::order::Class::Bracket,
        side: apca::api::v2::order::Side::Buy,
        time_in_force: apca::api::v2::order::TimeInForce::UntilCanceled,
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
fn apca_bracket_order() -> Order {
    let mut order = apca_order();

    let mut stop = apca_order();
    stop.side = apca::api::v2::order::Side::Sell;
    stop.type_ = apca::api::v2::order::Type::Stop;
    stop.filled_quantity = Num::from(0);
    stop.filled_at = None;
    stop.stop_price = Some(Num::from(99));
    stop.status = apca::api::v2::order::Status::Held;

    let mut limit = apca_order();
    limit.side = apca::api::v2::order::Side::Sell;
    limit.type_ = apca::api::v2::order::Type::Limit;
    limit.filled_quantity = Num::from(0);
    limit.filled_at = None;
    limit.limit_price = Some(Num::from(103));
    limit.status = apca::api::v2::order::Status::New;

    order.legs = vec![limit, stop];
    order
}
