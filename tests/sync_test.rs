use num_decimal::Num;
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

// this really tests that turbosql is working, but there were ... <issues> ... with the enum.
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
fn test_sync_trade_update() {
    setup();
    let lot = create_lot();
    assert!(lot.rowid > Some(0));
    assert_eq!(lot.sym.unwrap(), "TEST");
    assert_eq!(lot.status.unwrap(), LotStatus::Pending);
}
