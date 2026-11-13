use super::*;

/// A box 40×10 under `id`.
fn sized(ops: &mut Vec<Op>, id: ViewId) {
    let mut s = StyleProps::default();
    s.mask = mask(&[StyleId::Width, StyleId::Height]);
    s.width = Dimension::Points(40.);
    s.height = Dimension::Points(10.);
    ops.push(Op::CreateView {
        id,
        node_type: NodeType::View,
    });
    ops.push(Op::SetStyle {
        id,
        patch: Box::new(s),
    });
}

/// A region lays a titled alert out as ordinary layout does: the title's
/// box first, the alert's children below it.
#[test]
fn a_region_lays_a_titled_alert_out_as_ordinary_layout() {
    use exact_kernel::region::RegionProfile;
    let (child, alert, after) = (10, 11, 12);
    for profile in [RegionProfile::PinnedOffers, RegionProfile::SplitFacts] {
        let mut k = fixture();
        let mut ops = Vec::new();
        sized(&mut ops, child);
        sized(&mut ops, after);
        ops.extend([
            Op::CreateView {
                id: alert,
                node_type: NodeType::View,
            },
            Op::SetProp {
                id: alert,
                prop: PropId::AccessibilityLabel,
                value: "Delete the draft?".into(),
            },
            Op::SetChildren {
                id: alert,
                children: vec![child],
            },
            Op::SetChildren {
                id: 3,
                children: vec![alert, after],
            },
        ]);
        k.apply(0, 0, &ops).unwrap();
        k.set_alert_prefix(alert, true).unwrap();
        k.set_content_region_profile(Some(binding(&k)), profile)
            .unwrap();
        assert!(ready(&mut k, 400., 1).current, "{profile:?}");
        let mut oracle = k.rehydrate(Box::new(MonospaceMeasurer::default()));
        oracle
            .compute_layout(1, Offer::definite(400., 300.))
            .unwrap();
        let (top, below) = (
            oracle.node(alert).unwrap().frame.y,
            oracle.node(child).unwrap().frame.y,
        );
        assert!(below > top, "the oracle sets the title above the child");
        for id in [child, alert, after] {
            assert!(
                k.node(id)
                    .unwrap()
                    .frame
                    .bits_eq(oracle.node(id).unwrap().frame),
                "{profile:?}: {id} at {:?}, not {:?}",
                k.node(id).unwrap().frame,
                oracle.node(id).unwrap().frame
            );
        }
    }
}
