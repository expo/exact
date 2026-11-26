//! The cursor's corrections and the refinement queue.
use super::*;
use exact_runner::AnchorCorrection;

fn snapshot() -> CollectionSnapshot {
    CollectionSnapshot {
        window: None,
        fixed: Vec::new(),
        view: 1,
        axis: exact_runner::ListAxis::Vertical,
        parent: None,
        restored: false,
        seeking: false,
        waiting: false,
        request: 0,
        revision: 4,
        scroll_sequence: 2,
        count: 0,
        total_extent: 0.,
        rows: vec![],
        correction: Some(AnchorCorrection {
            scroll_sequence: 2,
            offset: 200.,
            from: None,
            smooth: false,
        }),
        pending: false,
    }
}
#[test]
fn corrections_are_consumed_once_and_user_scroll_or_resize_wins() {
    let s = snapshot();
    let mut cursor = Cursor {
        sequence: 2,
        ..Cursor::default()
    };
    assert_eq!(cursor.correction(&s, None), Some(200.));
    assert_eq!(cursor.correction(&s, None), None);
    let mut cursor = Cursor {
        sequence: 2,
        ..Cursor::default()
    };
    cursor.advance();
    assert_eq!(cursor.correction(&s, None), None);
    let mut cursor = Cursor {
        sequence: 2,
        ..Cursor::default()
    };
    cursor.geometry((200., 300., 180., [10., 0.]));
    cursor.geometry((190., 300., 170., [10., 0.]));
    assert_eq!(cursor.correction(&s, None), None);
    let mut cursor = Cursor {
        sequence: 2,
        ..Cursor::default()
    };
    cursor.geometry((200., 300., 180., [10., 0.]));
    cursor.geometry((200., 300., 180., [20., 0.]));
    assert_eq!(
        cursor.correction(&s, None),
        None,
        "origin changes also invalidate old corrections"
    );
    // A resize in the pass that brings the correction (feed F14) is
    // not the reader: the correction planned before it lands.
    let mut cursor = Cursor {
        sequence: 2,
        ..Cursor::default()
    };
    cursor.geometry((200., 300., 180., [10., 0.]));
    cursor.geometry((200., 348., 180., [10., 0.]));
    assert_eq!(cursor.correction(&s, Some(2)), Some(200.));
    let mut cursor = Cursor {
        sequence: u64::MAX,
        ..Cursor::default()
    };
    let mut overflow = s;
    overflow.correction.as_mut().unwrap().scroll_sequence = u64::MAX;
    assert_eq!(cursor.correction(&overflow, None), None);
    // A request's move lands though the reader has scrolled since the
    // report it was planned on; an anchor's relative one does not.
    let mut seeking = snapshot();
    seeking.seeking = true;
    let mut cursor = Cursor {
        sequence: 2,
        ..Cursor::default()
    };
    cursor.advance();
    assert_eq!(cursor.correction(&seeking, None), Some(200.));
    seeking.revision += 1;
    seeking.correction.as_mut().unwrap().from = Some(150.);
    assert_eq!(cursor.correction(&seeking, None), None);
    // Sent again after the reader moved the port (a data update before
    // the next report), it does not land; a newer request's move does.
    seeking.revision += 1;
    seeking.correction.as_mut().unwrap().from = None;
    cursor.reader_moved = true;
    cursor.advance();
    assert_eq!(cursor.correction(&seeking, None), None);
    seeking.revision += 1;
    seeking.request += 1;
    assert_eq!(cursor.correction(&seeking, None), Some(200.));
}

#[test]
fn refinement_queue_deduplicates_and_forgets_unmounted_collections() {
    let mut state = State::default();
    for _ in 0..1000 {
        state.schedule(&[snapshot()]);
    }
    assert_eq!(state.queue.len(), 1);
    assert_eq!(state.cursors.len(), 1);
    state.schedule(&[]);
    assert!(!state.pending());
    assert!(state.cursors.is_empty());
}

#[test]
fn empty_collection_padding_resolves_lengths_and_percentages() {
    struct Empty;
    impl DataSource for Empty {
        fn query(
            &mut self,
            _: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Ok(exact_runner::Value::list(vec![]))
        }
    }
    for (padding, cross) in [
        ("20px", 60.),
        ("10%", 60.),
        ("calc(10% + 3px)", 54.),
        ("min(10%, 12px)", 76.),
        ("clamp(10px, 10%, 30px)", 60.),
    ] {
        let source = format!("component App\n  resource rows = rows() as shape number[]\n  view\n    column width=200\n      list virtualized=true width=\"50%\" height=100 padding-left=\"{padding}\" padding-right=\"{padding}\" box-sizing=\"border-box\"\n        for x in rows key=x\n          text `${{x}}`\n");
        let plan = contract::compile(&source).unwrap();
        let (host, error) = Host::boot(
            &plan.encode(),
            Empty,
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.,
            500.,
        )
        .unwrap();
        assert!(error.is_none());
        let facts = geometry(host.kernel(), &host.collections()[0], 400., None).unwrap();
        assert_eq!(facts.width, 100., "{padding}");
        assert_eq!(facts.cross, cross, "{padding}");
        assert_eq!(facts.origin, 0., "{padding}");
    }
}

/// A padding whose `calc()` resolves below zero is zero, CSS's range for
/// padding (LLP 1010 §6.9): the origin, the range and the reported
/// padding all take 0, on either axis, and the runner accepts the report.
#[test]
fn a_negative_resolved_padding_is_zero_on_either_axis() {
    struct Rows;
    impl DataSource for Rows {
        fn query(
            &mut self,
            _: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Ok(exact_runner::Value::list(
                (0..50)
                    .map(|i| exact_runner::Value::Number(i as f64))
                    .collect(),
            ))
        }
    }
    let below = "\"calc(env(safe-area-inset-bottom) - 12px)\"";
    for list in [
        format!("list virtualized=true width=200 height=100 overflow-x=\"hidden\" padding-top={below} padding-bottom={below}"),
        format!("list virtualized=true display=\"flex\" width=200 height=100 overflow-x=\"scroll\" overflow-y=\"hidden\" padding-left={below} padding-right={below}"),
    ] {
        let source = format!("component App\n  resource rows = rows() as shape number[]\n  view\n    column width=200\n      {list}\n        for x in rows key=x\n          text `${{x}}` width=40 height=20\n");
        let plan = contract::compile(&source).unwrap();
        let (mut host, error) = Host::boot(
            &plan.encode(),
            Rows,
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.,
            500.,
        )
        .unwrap();
        assert!(error.is_none());
        let snapshot = host.collections()[0].clone();
        let facts = geometry(host.kernel(), &snapshot, 400., None).unwrap();
        assert_eq!((facts.origin, facts.padding), (0., [0.; 2]), "{list}");
        let feedback = CollectionFeedback {
            view: snapshot.view,
            revision: snapshot.revision,
            scroll_sequence: 1,
            offset: 0.,
            port_main: facts.main(),
            port_cross: facts.port_cross(),
            cross: facts.cross,
            padding: facts.padding,
            measurements: vec![],
            focus_view: None,
            interaction_view: None,
        };
        host.collection_feedback_filled(feedback, CollectionFill::default())
            .unwrap_or_else(|e| panic!("{list}: {e:?}"));
    }
}

/// @ref LLP 1123 D14 — a claimed list reports its reached scrollport as its port, its insets in its padding before and
/// after the content, and the range that gives: a 100-point list reaching 40 before and 60 after with insets of 40 and
/// 100 is a 200-point port with padding 40 and 100, and 40 more of range; on either axis. A negative inset is a negative
/// padding, which the runner's feedback takes.
#[test]
fn a_claimed_list_reports_its_reached_port_and_its_insets_as_padding() {
    struct Rows;
    impl DataSource for Rows {
        fn query(
            &mut self,
            _: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Ok(exact_runner::Value::list(
                (0..50)
                    .map(|i| exact_runner::Value::Number(i as f64))
                    .collect(),
            ))
        }
    }
    let vertical = crate::safe_area::ScrollClaim {
        reach: [40., 0., 60., 0.],
        insets: [40., 0., 100., 0.],
    };
    let horizontal = crate::safe_area::ScrollClaim {
        reach: [0., 60., 0., 40.],
        insets: [0., 100., 0., 40.],
    };
    for (list, claim) in [
        ("list virtualized=true width=200 height=100", vertical),
        ("list virtualized=true display=\"flex\" width=100 height=200 overflow-x=\"scroll\" overflow-y=\"hidden\"", horizontal),
    ] {
        let source = format!("component App\n  resource rows = rows() as shape number[]\n  view\n    column width=200\n      {list}\n        for x in rows key=x\n          text `${{x}}` width=20 height=20\n");
        let plan = contract::compile(&source).unwrap();
        let (host, error) = Host::boot(&plan.encode(), Rows, Box::new(exact_kernel::MonospaceMeasurer::default()), 400., 500.)
            .unwrap();
        assert!(error.is_none());
        let snapshot = host.collections()[0].clone();
        let plain = geometry(host.kernel(), &snapshot, 400., None).unwrap();
        let claimed = geometry(host.kernel(), &snapshot, 400., Some(claim)).unwrap();
        assert_eq!((plain.main(), plain.origin, plain.padding), (100., 0., [0.; 2]), "{list}");
        assert_eq!((claimed.main(), claimed.origin, claimed.padding), (200., 40., [40., 100.]), "{list}");
        // The insets add 140 to the range and the reach takes 100 off it
        assert_eq!(claimed.max, plain.max + 40., "{list}");
        assert_eq!(claimed.port_cross(), plain.port_cross(), "the cross axis as laid out");
        // A negative inset is a negative padding, which the runner takes: the range starts 10 past the first row
        let mut negative = claim;
        negative.insets = negative.insets.map(|_| 0.);
        negative.insets[if list.contains("flex") { 3 } else { 0 }] = -10.;
        let g = geometry(host.kernel(), &snapshot, 400., Some(negative)).unwrap();
        assert_eq!(g.padding, [-10., 0.], "{list}");
        let feedback = CollectionFeedback {
            view: snapshot.view,
            revision: snapshot.revision,
            scroll_sequence: 1,
            offset: (0. - g.origin).max(-g.origin),
            port_main: g.main(),
            port_cross: g.port_cross(),
            cross: g.cross,
            padding: g.padding,
            measurements: vec![],
            focus_view: None,
            interaction_view: None,
        };
        assert_eq!(feedback.offset, 10.);
        let mut host = host;
        host.collection_feedback_filled(feedback, CollectionFill::default())
            .unwrap_or_else(|e| panic!("{list}: {e:?}"));
    }
}

/// @ref LLP 1123 D14 — a claimed list that follows its end follows the claim's: at its end when a claim reaching 40
/// above and 60 below with insets 40 and 100 comes, it follows to that claim's end, 40 past the one it was at, and
/// stays at the end as its padding grows under the claim; released, it follows back to the unclaimed end.
#[test]
fn a_claimed_list_follows_the_claims_end() {
    struct Rows;
    impl DataSource for Rows {
        fn query(
            &mut self,
            _: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Ok(exact_runner::Value::list(
                (0..20)
                    .map(|i| exact_runner::Value::Number(i as f64))
                    .collect(),
            ))
        }
    }
    let source = r#"component App
  resource rows = rows() as shape number[]
  state end = 83
  action grow
    end = end + 26
  view
    column
      button "Grow" press=grow testId="grow"
      list virtualized=true scrollFollowEnd=true height=180 width=200 padding-bottom=end box-sizing="border-box" testId="feed"
        for x in rows key=x
          text `row ${x}` height=24
"#;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        Rows,
        (400., 500.),
        1.,
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let settle = |p: &mut Presenter<Rows>| {
        for _ in 0..16 {
            assert!(p.pump(p.host.now()).is_none());
            if p.dirty() {
                let _ = p.frame();
            }
        }
    };
    let named = |p: &Presenter<Rows>, name: &str| {
        let key = p.host.kernel().find_by_test_id(name)[0];
        p.host.kernel().node_by_key(key).unwrap().id
    };
    settle(&mut p);
    let view = named(&p, "feed");
    p.wheel(view, 0., 10_000_000.).unwrap();
    settle(&mut p);
    let end = p.collection_scroll_limits()[&view];
    assert_eq!(p.scroll_of(view).1, end, "at the end");
    assert!(p.set_scroll_claim(view, Some(([40., 0., 60., 0.], [40., 0., 100., 0.]))));
    settle(&mut p);
    // Every row measured, its range is 40 more: its insets' 140 less its reach's 100
    let claimed = p.collection_scroll_limits()[&view];
    assert_eq!(claimed, end + 40., "the claim's range");
    assert_eq!(p.scroll_of(view).1, claimed, "followed to the claim's end");
    p.tap(named(&p, "grow")).unwrap();
    settle(&mut p);
    // The padding after the rows grows by 26 under the claim's 100
    let snapshot = p
        .host
        .collections_shallow()
        .into_iter()
        .find(|s| s.view == view)
        .unwrap();
    let g = geometry(p.host.kernel(), &snapshot, 400., p.list_claim(&snapshot)).unwrap();
    assert_eq!(g.padding, [40., 209.]);
    let grown = p.collection_scroll_limits()[&view];
    assert_eq!(grown, claimed + 26.);
    assert_eq!(
        p.scroll_of(view).1,
        grown,
        "followed to the grown end under the claim"
    );
    assert!(p.set_scroll_claim(view, None));
    settle(&mut p);
    let released = p.collection_scroll_limits()[&view];
    assert_eq!(released, grown - 40.);
    assert_eq!(
        p.scroll_of(view).1,
        released,
        "followed back to the unclaimed end"
    );
    // The runner heard of the release at once: a wheel moves the list from there
    p.wheel(view, 0., -100.).unwrap();
    settle(&mut p);
    assert_eq!(p.scroll_of(view).1, released - 100.);
}
