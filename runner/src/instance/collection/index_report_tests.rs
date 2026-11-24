//! A report's anchor (`SizeIndex::capture_report_anchor`).
use super::*;

use super::tests::{index, keys, near};

/// A report's anchor (`capture_report_anchor`): a row the report measures for
/// the first time, the port's top inside it, yields the place to the next row
/// the port shows, its top at the port's top included; one the report does not
/// list keeps it, as does a listed row with no other row to yield to.
#[test]
fn a_report_anchors_past_a_row_it_measures_for_the_first_time() {
    // `k0` to `k4` measured at 20, then `new` before them at the estimate, 10
    let mut keyed: Vec<Rc<str>> = vec![Rc::from("new")];
    keyed.extend(keys(5));
    let landed = || {
        let mut i = index(&[20.; 5]);
        i.replace_keys(keyed.clone()).unwrap();
        i
    };
    let first = |i: &mut SizeIndex, key: &str, height: f64| {
        let token = i.measurement_token(key).unwrap();
        assert!(i.set_measured_height(key, token, height).unwrap());
    };
    let restored = |offset: f64, measured: &[&str]| {
        let mut i = landed();
        let anchor = i
            .capture_report_anchor(offset, 30., false, measured)
            .unwrap();
        first(&mut i, "new", 30.);
        i.restore_anchor(&anchor, 30.).unwrap()
    };
    // The port's top 5 into `new`: `k0`, at 5 below it, stays there as `new` grows 20
    near(restored(5., &["new"]), 25.);
    near(restored(5., &[]), 5.);
    // `new`'s top at the port's but for a device pixel: `k0` holds the place all the same
    near(restored(0.33, &["new"]), 20.33);
    // Every row the port shows measured for the first time: the first anchor, not the offset
    let mut fresh = SizeIndex::new(10.0).unwrap();
    fresh.replace_keys(keys(5)).unwrap();
    let all = ["k0", "k1", "k2", "k3", "k4"];
    let anchor = fresh.capture_report_anchor(5., 30., false, &all).unwrap();
    assert_eq!(anchor.row, Some(0));
    for key in all {
        first(&mut fresh, key, 40.);
    }
    near(fresh.restore_anchor(&anchor, 30.).unwrap(), 5.);
}

/// A first-measured row whose top is exactly the port's yields the place too:
/// `k0` measured at 20, then `new` after it at the estimate, 10, the port's top
/// at `new`'s; `k1`, 10 below the port's top, stays there as `new` grows 20.
#[test]
fn a_first_measured_row_at_the_ports_top_yields_the_place() {
    let mut keyed: Vec<Rc<str>> = vec![Rc::from("k0"), Rc::from("new")];
    keyed.extend(keys(5).into_iter().skip(1));
    let mut i = index(&[20.; 5]);
    i.replace_keys(keyed).unwrap();
    let anchor = i.capture_report_anchor(20., 30., false, &["new"]).unwrap();
    let token = i.measurement_token("new").unwrap();
    assert!(i.set_measured_height("new", token, 30.).unwrap());
    near(i.restore_anchor(&anchor, 30.).unwrap(), 40.);
}

/// A report passes a first-measured row by its index: behind a 100 px lead,
/// the row's end with the lead added and taken away again (100 + 32.1 - 100
/// is 32.099999999999994) is inside the row, and the next row holds the place.
#[test]
fn a_report_passes_a_first_measured_row_behind_a_lead() {
    let mut i = SizeIndex::new(32.1).unwrap();
    i.replace_keys(keys(5)).unwrap();
    i.measure_fixed(0, 100.).unwrap();
    let anchor = i.capture_report_anchor(116., 100., false, &["k0"]).unwrap();
    assert_eq!(anchor.row, Some(1));
    // A hidden row after it is passed too, as the lookup passes one: `k1` at 0, `k2` first measured, `k3` holds it
    let token = i.measurement_token("k1").unwrap();
    assert!(i.set_measured_height("k1", token, 0.).unwrap());
    let anchor = i
        .capture_report_anchor(116., 100., false, &["k0", "k2"])
        .unwrap();
    assert_eq!(anchor.row, Some(3));
}

/// A row that is only its gap shows nothing and holds no place: cards of 20
/// with 10 between, `k1` measured at no width, `k0` and `k2` first measured;
/// `k3` holds it as `k2` grows 20.
#[test]
fn a_report_passes_a_row_that_is_only_its_gap() {
    let mut i = SizeIndex::new(30.).unwrap();
    i.replace_keys(keys(10)).unwrap();
    i.resize(30., 10., &[]).unwrap();
    let measure = |i: &mut SizeIndex, key: &str, size: f64| {
        let token = i.measurement_token(key).unwrap();
        assert!(i.set_measured_height(key, token, size).unwrap());
    };
    measure(&mut i, "k1", 10.);
    for key in keys(10).iter().skip(3) {
        measure(&mut i, key, 30.);
    }
    let anchor = i
        .capture_report_anchor(10., 100., false, &["k0", "k2"])
        .unwrap();
    assert_eq!(anchor.row, Some(3));
    measure(&mut i, "k0", 30.);
    measure(&mut i, "k2", 50.);
    near(i.restore_anchor(&anchor, 100.).unwrap(), 30.);
}

/// The mark a report reads (`SizeIndex::estimated`): a new row's height is the
/// estimate, and so is the height of each row a new estimate resets; a
/// measurement or a previous plan's measured size clears the mark, and an
/// invalidation leaves it as it was.
#[test]
fn a_row_is_estimated_until_a_layout_measures_it() {
    let mut i = index(&[20., 20.]);
    assert!(!i.estimated(0) && !i.estimated(1));
    let mut keyed: Vec<Rc<str>> = vec![Rc::from("new")];
    keyed.extend(keys(2));
    i.replace_keys(keyed).unwrap();
    assert!(i.estimated(0) && !i.estimated(1));
    i.invalidate_row("k0").unwrap();
    assert!(!i.estimated(1), "an edit leaves a measured row measured");
    i.seed_height("new", 30.).unwrap();
    assert!(!i.estimated(0), "a previous plan's measured size");
    i.resize(12., 0., &[1]).unwrap();
    assert!(
        i.estimated(0) && !i.estimated(1) && i.estimated(2),
        "a new estimate resets the rows it does not keep"
    );
}
