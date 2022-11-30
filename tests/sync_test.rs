use turbosql::{execute, select, Turbosql};
use zoocarp::{Lot, LotStatus};

#[cfg(test)]
fn setup() {
    let _res = std::panic::catch_unwind(|| execute!("DELETE FROM lot").unwrap());
}

// this really tests that turbosql is working, but there were ... <issues> ... with the enum.
#[test]
fn test_lot_can_be_saved_and_fetched() {
    setup();
    let lot = Lot::default_for_test();
    let rowid = lot.insert().unwrap();
    assert!(rowid > 0);
    let lot = Lot::get(rowid).unwrap();
    assert_eq!(lot.sym.unwrap(), "TEST");
    assert_eq!(lot.status.unwrap(), LotStatus::Open);

    let mut lot2 = Lot::default_for_test();
    lot2.status = Some(LotStatus::Canceled);
    let rowid = lot2.insert().unwrap();
    assert!(rowid > 1);

    let lots = select!(Vec<Lot> "WHERE status = ?", LotStatus::Open).unwrap();

    assert_eq!(lots.len(), 1);
}
