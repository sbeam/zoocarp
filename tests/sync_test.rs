use num_decimal::Num;
use std::fs::read_to_string;
use turbosql::{execute, select, Turbosql};
use zoocarp::{sync_lots::*, Lot, LotStatus, OrderTimeInForce, PositionType};

#[cfg(test)]
fn setup() {
    let _res = std::panic::catch_unwind(|| execute!("DELETE FROM lot").unwrap());
}

#[cfg(test)]
fn create_lot() -> Lot {
    let rowid = Lot::create(
        "TEST".to_string(),
        Num::from(11),
        PositionType::Long,
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
