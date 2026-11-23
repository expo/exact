//! A multi-column box's used `column-width` is at least 1px (CSS Multi-column
//! Layout 1 §3.1). At zero beside a zero gap the column count was u32::MAX and
//! layout did not return.

use crate::source::{boxed, NoData};
use exact_kernel::{Kernel, Offer};
use exact_runner::Runner;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_zero_column_width_lays_out_as_one_pixel_columns() {
    let (sent, got) = mpsc::channel();
    std::thread::spawn(move || {
        let src = "component App\n  view\n    view column-width=0 column-gap=0 width=400\n      view height=20\n";
        let plan = contract::compile(src).unwrap();
        let mut r = Runner::boot(
            plan,
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let root = r.kernel().roots()[0];
        r.kernel_mut()
            .compute_layout(root, Offer::definite(400.0, 800.0))
            .unwrap();
        sent.send(()).unwrap();
    });
    got.recv_timeout(Duration::from_secs(60))
        .expect("layout returned");
}
